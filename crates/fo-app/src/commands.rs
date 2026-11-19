//! Tauri commands. Logic lives in plain `*_impl` functions so it can be
//! unit-tested without a Tauri runtime; the commands are thin wrappers.

use crate::state::AppState;
use fo_core::registry::Registry;
use fo_types::{DetectedFile, FileKind, JobEvent, JobRequest, Platform, ToolInfo};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub const JOB_EVENT: &str = "job-event";
pub const FILES_DROPPED: &str = "files-dropped";

/// Serialised as `{"ok": …}` or `{"err": "…"}` for the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Detected {
    Ok(DetectedFile),
    Err(String),
}

pub fn current_platform() -> Platform {
    if cfg!(target_os = "android") {
        Platform::Android
    } else {
        Platform::Desktop
    }
}

pub fn detect_files_impl(paths: Vec<String>) -> Vec<Result<DetectedFile, String>> {
    paths
        .into_iter()
        .map(|p| {
            let path = PathBuf::from(&p);
            fo_core::detect::detect_file(&path).map_err(|e| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(p.clone());
                match e.kind() {
                    std::io::ErrorKind::NotFound => format!("File not found: {name}"),
                    std::io::ErrorKind::PermissionDenied => format!("No permission to read {name}"),
                    _ => format!("Couldn't read {name}: {e}"),
                }
            })
        })
        .collect()
}

pub fn tools_for_impl(registry: &Registry, kind: &FileKind, platform: Platform) -> Vec<ToolInfo> {
    registry.tools_for(kind, platform)
}

pub fn default_out_dir_impl(documents: &Path) -> Result<String, String> {
    let dir = documents.join("FileOdyssey");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    Ok(dir.to_string_lossy().into_owned())
}

pub fn check_openable(placed: &crate::state::Placed, path: &str) -> Result<(), String> {
    if placed.contains(path) {
        Ok(())
    } else {
        Err("Only results made in this session can be opened".into())
    }
}

#[tauri::command]
pub fn platform() -> Platform {
    current_platform()
}

#[tauri::command]
pub fn detect_files(paths: Vec<String>) -> Vec<Detected> {
    detect_files_impl(paths)
        .into_iter()
        .map(|r| match r {
            Ok(f) => Detected::Ok(f),
            Err(e) => Detected::Err(e),
        })
        .collect()
}

#[tauri::command]
pub fn tools_for(state: State<'_, AppState>, kind: FileKind) -> Vec<ToolInfo> {
    tools_for_impl(&state.registry, &kind, current_platform())
}

#[tauri::command]
pub fn default_out_dir(app: AppHandle) -> Result<String, String> {
    let base = app
        .path()
        .document_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("No documents folder: {e}"))?;
    default_out_dir_impl(&base)
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Vec<String> {
    app.dialog()
        .file()
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.to_string())
        .collect()
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    pick_folder_impl(&app)
}

#[cfg(desktop)]
fn pick_folder_impl(app: &AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|p| p.to_string())
}

/// The dialog plugin has no folder picker on Android; results always go to
/// the default folder there and the UI hides "Change".
#[cfg(mobile)]
fn pick_folder_impl(_app: &AppHandle) -> Option<String> {
    None
}

#[tauri::command]
pub fn start_job(app: AppHandle, state: State<'_, AppState>, req: JobRequest) -> String {
    let placed = state.placed.clone();
    let sink = Arc::new(move |ev: JobEvent| {
        if let fo_types::JobStatus::Done { outputs, .. } = &ev.status {
            placed.record(outputs);
        }
        let _ = app.emit(JOB_EVENT, ev);
    });
    state.runner.submit(req, sink)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) {
    state.runner.cancel(&job_id);
}

#[tauri::command]
pub fn open_path(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    check_openable(&state.placed, &path)?;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    check_openable(&state.placed, &path)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fo_types::{FileKind, Platform};
    use tempfile::TempDir;

    #[test]
    fn detect_files_mixes_ok_and_err() {
        let d = TempDir::new().unwrap();
        let real = d.path().join("a.md");
        std::fs::write(&real, "# hi").unwrap();
        let missing = d.path().join("gone.pdf");
        let got = detect_files_impl(vec![
            real.to_string_lossy().into(),
            missing.to_string_lossy().into(),
        ]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].as_ref().unwrap().kind, FileKind::Markdown);
        assert_eq!(got[1].as_ref().unwrap_err(), "File not found: gone.pdf");
    }

    #[test]
    fn default_out_dir_created_under_documents() {
        let d = TempDir::new().unwrap();
        let out = default_out_dir_impl(d.path()).unwrap();
        assert_eq!(out, d.path().join("FileOdyssey").to_string_lossy());
        assert!(d.path().join("FileOdyssey").is_dir());
    }

    #[test]
    fn csp_allows_wasm_ui() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().unwrap();
        let script = csp
            .split(';')
            .map(str::trim)
            .find(|d| d.starts_with("script-src"))
            .expect("script-src");
        assert!(
            script.contains("'self'") && script.contains("'wasm-unsafe-eval'"),
            "{script}"
        );
        // The UI fetches its own .wasm file, which falls under connect-src.
        let connect = csp
            .split(';')
            .map(str::trim)
            .find(|d| d.starts_with("connect-src"))
            .expect("connect-src");
        assert!(connect.contains("'self'"), "{connect}");
        assert!(
            !csp.contains("http://") || csp.contains("http://ipc.localhost"),
            "no remote origins: {csp}"
        );
    }

    #[test]
    fn only_results_from_this_session_can_be_opened() {
        let placed = crate::state::Placed::default();
        placed.record(&["/out/a (converted).docx".to_string()]);
        assert!(check_openable(&placed, "/out/a (converted).docx").is_ok());
        assert_eq!(
            check_openable(&placed, "C:\\Windows\\System32\\calc.exe").unwrap_err(),
            "Only results made in this session can be opened"
        );
    }

    #[test]
    fn tools_for_unknown_is_empty() {
        let reg = fo_core::registry::Registry::new();
        assert!(tools_for_impl(&reg, &FileKind::Unknown, Platform::Desktop).is_empty());
    }
}
