//! Reader commands and the `reader://` protocol. All logic is in
//! `fo_core::reader::session`; these are thin, non-blocking wrappers.

use crate::state::AppState;
use fo_types::{ReaderView, SearchHit, TextSpan};
use tauri::http::{header, Request, Response};
use tauri::{State, UriSchemeContext, UriSchemeResponder};

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "The reader stopped unexpectedly".to_string())?
}

#[tauri::command]
pub async fn reader_open(state: State<'_, AppState>, path: String) -> Result<ReaderView, String> {
    let s = state.reader.clone();
    blocking(move || s.open(std::path::Path::new(&path))).await
}

#[tauri::command]
pub async fn reader_chapter(
    state: State<'_, AppState>,
    doc: u64,
    index: usize,
) -> Result<String, String> {
    let s = state.reader.clone();
    blocking(move || s.chapter(doc, index)).await
}

#[tauri::command]
pub async fn reader_text(
    state: State<'_, AppState>,
    doc: u64,
    page: u32,
) -> Result<Vec<TextSpan>, String> {
    let s = state.reader.clone();
    blocking(move || s.text_spans(doc, page)).await
}

#[tauri::command]
pub async fn reader_search(
    state: State<'_, AppState>,
    doc: u64,
    query: String,
) -> Result<Vec<SearchHit>, String> {
    let s = state.reader.clone();
    blocking(move || s.search(doc, &query)).await
}

#[tauri::command]
pub fn reader_close(state: State<'_, AppState>, doc: u64) {
    state.reader.close(doc);
}

/// Handler for `reader://` (`http://reader.localhost` on Windows/Android).
pub fn protocol<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    use tauri::Manager;
    let sessions = ctx.app_handle().state::<AppState>().reader.clone();
    let uri = request.uri();
    let path = match uri.query() {
        Some(q) => format!("{}?{}", uri.path(), q),
        None => uri.path().to_owned(),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let built = match sessions.serve(&path) {
            Ok((bytes, mime)) => Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "private, max-age=600")
                .body(bytes),
            Err((status, msg)) => Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(msg.into_bytes()),
        };
        if let Ok(resp) = built {
            responder.respond(resp);
        }
    });
}
