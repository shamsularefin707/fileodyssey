//! FileOdyssey's Android-only calls, implemented in Kotlin
//! (`android/src/main/java/FoAndroidPlugin.kt`) and invoked from the
//! webview as `plugin:fo-android|<command>`:
//!
//! - `scanner_available` → `{ available }`: Google Play services is present
//!   (the ML Kit document scanner needs it).
//! - `scan_document { dir, pageLimit }` → `{ pages: [path…] }`: runs the ML Kit
//!   document scanner and copies each page's JPEG into `dir`. Rejects with
//!   code `cancelled` when the user backs out, `unavailable` when it can't run.
//! - `add_wifi { ssid, password, security }` → `{ mode }`: opens the system
//!   dialog that adds the network (Android 11+, `mode: "dialog"`) or the Wi-Fi
//!   settings (`mode: "settings"`).
//!
//! On other platforms the plugin registers nothing.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("fo-android")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _api.register_android_plugin("app.fileodyssey.android", "FoAndroidPlugin")?;
            Ok(())
        })
        .build()
}
