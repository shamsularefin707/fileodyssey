mod commands;
mod pandoc;
mod reader;
mod state;

use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::new())
        .register_asynchronous_uri_scheme_protocol("reader", reader::protocol)
        .setup(|app| {
            app.state::<state::AppState>()
                .pandoc
                .attach(app.handle().clone());
            // Bundled PDFium lives in the app's resources on desktop.
            if let Ok(dir) = app.path().resource_dir() {
                app.state::<state::AppState>()
                    .reader
                    .add_lib_dir(dir.join("pdfium"));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                let _ = window.emit(commands::FILES_DROPPED, paths);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform,
            commands::detect_files,
            commands::tools_for,
            commands::default_out_dir,
            commands::pick_files,
            commands::pick_folder,
            commands::start_job,
            commands::cancel_job,
            commands::open_path,
            commands::reveal_path,
            commands::open_libreoffice_download,
            reader::reader_open,
            reader::reader_chapter,
            reader::reader_text,
            reader::reader_search,
            reader::reader_close,
            pandoc::pandoc_take,
            pandoc::pandoc_done,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FileOdyssey");
}
