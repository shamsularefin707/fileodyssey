const COMMANDS: &[&str] = &["scanner_available", "scan_document", "add_wifi"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
