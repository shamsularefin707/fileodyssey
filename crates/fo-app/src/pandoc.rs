//! Pandoc inside the webview.
//!
//! The official pandoc.wasm needs a wasm engine with legacy exception
//! handling, which every webview has and wasmtime doesn't, so conversions
//! run in a Web Worker (`crates/fo-ui/pandoc-worker`). A job thread calls
//! `run`: the request frame is parked here, the UI is told its id, fetches
//! it with `pandoc_take`, and hands the result back with `pandoc_done`.

use fo_core::pandoc::frame::{decode_result, encode_request};
use fo_core::pandoc::{PandocEngine, PandocRequest, PandocResult};
use fo_core::tool::CancelToken;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const REQUEST: &str = "pandoc-request";
pub const CANCEL: &str = "pandoc-cancel";
/// A conversion that takes longer than this is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(600);

type Reply = Result<PandocResult, String>;

#[derive(Default)]
pub struct WebviewPandoc {
    app: OnceLock<AppHandle>,
    frames: Mutex<HashMap<u64, Vec<u8>>>,
    waiters: Mutex<HashMap<u64, Sender<Reply>>>,
    next: AtomicU64,
}

#[derive(serde::Serialize, Clone)]
struct Id {
    id: u64,
}

impl WebviewPandoc {
    pub fn attach(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    /// The request frame for `id`, once (the UI calls this).
    pub fn take(&self, id: u64) -> Option<Vec<u8>> {
        self.frames.lock().expect("frames").remove(&id)
    }

    /// The UI's result frame for `id`.
    pub fn done(&self, id: u64, frame: &[u8]) {
        let reply = decode_result(frame).and_then(|r| r);
        if let Some(tx) = self.waiters.lock().expect("waiters").remove(&id) {
            let _ = tx.send(reply);
        }
    }

    fn forget(&self, id: u64) {
        self.frames.lock().expect("frames").remove(&id);
        self.waiters.lock().expect("waiters").remove(&id);
        if let Some(app) = self.app.get() {
            let _ = app.emit(CANCEL, Id { id });
        }
    }

    /// Like `run`, with a custom limit (tests use a short one).
    pub fn run_with_timeout(
        &self,
        req: PandocRequest,
        cancel: &CancelToken,
        limit: Duration,
    ) -> Reply {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = channel();
        self.frames
            .lock()
            .expect("frames")
            .insert(id, encode_request(&req));
        self.waiters.lock().expect("waiters").insert(id, tx);
        match self.app.get() {
            Some(app) => {
                let _ = app.emit(REQUEST, Id { id });
            }
            None => {
                self.forget(id);
                return Err("The converter isn't ready yet".into());
            }
        }
        let start = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(r) => return r,
                Err(RecvTimeoutError::Timeout) => {
                    if cancel.is_cancelled() {
                        self.forget(id);
                        return Err("Cancelled".into());
                    }
                    if start.elapsed() > limit {
                        self.forget(id);
                        return Err(format!(
                            "The conversion took longer than {} minutes and was stopped",
                            limit.as_secs() / 60
                        ));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Err("The converter stopped".into()),
            }
        }
    }
}

impl PandocEngine for WebviewPandoc {
    fn run(&self, req: PandocRequest, cancel: &CancelToken) -> Reply {
        self.run_with_timeout(req, cancel, TIMEOUT)
    }
}

#[tauri::command]
pub fn pandoc_take(
    state: tauri::State<'_, crate::state::AppState>,
    id: u64,
) -> tauri::ipc::Response {
    tauri::ipc::Response::new(state.pandoc.take(id).unwrap_or_default())
}

#[tauri::command]
pub fn pandoc_done(
    state: tauri::State<'_, crate::state::AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let id = request
        .headers()
        .get("fo-job")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("missing job id")?;
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => state.pandoc.done(id, bytes),
        _ => state.pandoc.done(id, b""),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fo_core::pandoc::frame::{decode_request, encode_result};
    use serde_json::json;

    fn req() -> PandocRequest {
        PandocRequest {
            options: json!({"to": "html"}),
            files: vec![("a.md".into(), b"# A".to_vec())],
        }
    }

    #[test]
    fn without_a_webview_it_fails_cleanly() {
        let p = WebviewPandoc::default();
        assert!(p
            .run(req(), &CancelToken::new())
            .unwrap_err()
            .contains("isn't ready"));
        assert!(p.frames.lock().unwrap().is_empty());
    }

    #[test]
    fn take_and_done_answer_the_waiting_job() {
        let p = std::sync::Arc::new(WebviewPandoc::default());
        // Simulate the UI side by watching for parked frames.
        let ui = {
            let p = p.clone();
            std::thread::spawn(move || loop {
                let id = p.frames.lock().unwrap().keys().next().copied();
                if let Some(id) = id {
                    let r = decode_request(&p.take(id).unwrap()).unwrap();
                    assert_eq!(r.files[0].0, "a.md");
                    let res = PandocResult {
                        stdout: b"<h1>A</h1>".to_vec(),
                        ..Default::default()
                    };
                    p.done(id, &encode_result(&Ok(res)));
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            })
        };
        // Bypass the event emit (no app in tests) by parking directly.
        let id = 7;
        let (tx, rx) = channel();
        p.frames.lock().unwrap().insert(id, encode_request(&req()));
        p.waiters.lock().unwrap().insert(id, tx);
        let r = rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(r.stdout, b"<h1>A</h1>");
        ui.join().unwrap();
        p.done(id, b"late reply for a finished job"); // ignored
    }
}
