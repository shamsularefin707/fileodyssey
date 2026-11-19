mod commands;
mod state;

use tauri::{DragDropEvent, Emitter, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::new())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running FileOdyssey");
}
