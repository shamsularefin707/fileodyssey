//! Scan tab commands. The camera and the ML Kit scanner are driven from the
//! webview through their plugins; these commands do the rest: classify a
//! code, read codes from an image, save scanned pages, keep the history,
//! and the result actions (open a web link, save a contact).

use crate::state::AppState;
use fo_core::scan::{self, history::History};
use fo_types::{Payload, ScanKind, ScanRecord};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// The scan history, opened on first use in the app's data folder.
#[derive(Default)]
pub struct ScanStore(Mutex<Option<History>>);

fn scans_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("scans"))
        .map_err(|e| format!("No app data folder: {e}"))
}

fn with_history<T>(
    app: &AppHandle,
    store: &ScanStore,
    f: impl FnOnce(&mut History) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = store
        .0
        .lock()
        .map_err(|_| "Scan history is busy".to_string())?;
    if guard.is_none() {
        *guard = Some(History::open(&scans_dir(app)?));
    }
    f(guard.as_mut().expect("opened above"))
}

/// A fresh folder, owned by the history, for one document scan's pages.
#[tauri::command]
pub fn scan_pages_dir(app: AppHandle, store: State<'_, ScanStore>) -> Result<String, String> {
    with_history(&app, &store, |h| {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let dir = h.files_dir().join(stamp.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir.to_string_lossy().into_owned())
    })
}

/// A scanned page's thumbnail for the review screen. Only pages in the
/// history's own folder, so the webview can't read other files this way.
#[tauri::command]
pub async fn scan_thumbnail(
    app: AppHandle,
    store: State<'_, ScanStore>,
    path: String,
) -> Result<String, String> {
    let own = with_history(&app, &store, |h| Ok(h.files_dir()))?;
    let p = Path::new(&path).canonicalize().map_err(|e| e.to_string())?;
    let own = own.canonicalize().map_err(|e| e.to_string())?;
    if !p.starts_with(&own) {
        return Err("Not a scanned page".into());
    }
    tauri::async_runtime::spawn_blocking(move || scan::document::thumbnail(&p, 320))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn scan_classify(content: String) -> Payload {
    scan::payload::classify(&content)
}

#[tauri::command]
pub async fn scan_codes_in_image(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || scan::codes_in_image(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn scan_record_code(
    app: AppHandle,
    store: State<'_, ScanStore>,
    content: String,
    format: String,
) -> Result<ScanRecord, String> {
    with_history(&app, &store, |h| h.add(ScanKind::Code { content, format }))
}

/// Saves reviewed pages to `out_dir`, records the scan, and returns the
/// files written (they can be opened like any result).
#[tauri::command]
pub async fn scan_save_document(
    app: AppHandle,
    pages: Vec<String>,
    order: Vec<usize>,
    out_dir: String,
    stem: String,
    pdf: bool,
    jpegs: bool,
) -> Result<Vec<String>, String> {
    let pages: Vec<PathBuf> = pages.into_iter().map(PathBuf::from).collect();
    let pages2 = pages.clone();
    let files = tauri::async_runtime::spawn_blocking(move || {
        scan::document::save_scan(
            &pages2,
            &order,
            Path::new(&out_dir),
            &scan::safe_stem(&stem),
            pdf,
            jpegs,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    let names: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();
    app.state::<AppState>().placed.record(&names);
    let store = app.state::<ScanStore>();
    // The history keeps the originals (its own) and the saved files.
    let mut all = files;
    all.extend(pages);
    with_history(&app, &store, |h| h.add(ScanKind::Document { files: all }))?;
    Ok(names)
}

#[tauri::command]
pub fn scan_history(
    app: AppHandle,
    store: State<'_, ScanStore>,
) -> Result<Vec<ScanRecord>, String> {
    with_history(&app, &store, |h| Ok(h.list()))
}

#[tauri::command]
pub fn scan_delete(app: AppHandle, store: State<'_, ScanStore>, id: String) -> Result<(), String> {
    with_history(&app, &store, |h| h.delete(&id))
}

#[tauri::command]
pub fn scan_clear(app: AppHandle, store: State<'_, ScanStore>) -> Result<(), String> {
    with_history(&app, &store, |h| h.clear())
}

/// Opens a scanned web link, after the person has seen it and tapped Open.
/// Anything but http(s) is refused.
#[tauri::command]
pub fn scan_open_url(app: AppHandle, url: String) -> Result<(), String> {
    let url = scan::openable_url(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Saves a scanned contact as a .vcf in `out_dir` and opens it, so the
/// Contacts app can add it; returns the file.
#[tauri::command]
pub fn scan_save_contact(app: AppHandle, vcard: String, out_dir: String) -> Result<String, String> {
    let file = scan::save_contact(&vcard, Path::new(&out_dir))?;
    let name = file.to_string_lossy().into_owned();
    app.state::<AppState>()
        .placed
        .record(std::slice::from_ref(&name));
    // Opening hands it to the Contacts app; if nothing takes it, the file is
    // still saved and the UI says where.
    let _ = app.opener().open_path(&name, None::<&str>);
    Ok(name)
}
