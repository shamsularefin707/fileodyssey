//! Settings and updates. The update check is the app's only network use and
//! can be turned off. Desktop updates come through the Tauri updater
//! (signed manifests on the GitHub release); Android links to the release
//! (its check runs in the webview against GitHub's API).

use fo_types::{Settings, UpdateInfo};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// The only page the app opens for updates.
pub const RELEASES_URL: &str = "https://github.com/shamsularefin707/fileodyssey/releases/latest";

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("settings.json"))
        .map_err(|e| e.to_string())
}

/// Settings from `path`; defaults when missing or unreadable.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load(&settings_path(&app)?))
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    save(&settings_path(&app)?, &settings)
}

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// A newer version from the updater, if any (desktop). Errors (offline,
/// GitHub unreachable) come back as errors; the UI stays quiet about them
/// unless the person asked.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let update = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?;
        Ok(update.map(|u| UpdateInfo {
            version: u.version.clone(),
            notes: u.body.clone().unwrap_or_default(),
            installable: true,
        }))
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(None)
    }
}

/// Downloads, verifies (signature) and installs the update, then restarts.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let update = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("There's no newer version")?;
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| format!("Couldn't install the update: {e}"))?;
        app.restart();
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Err("Updates on Android come from the release page".into())
    }
}

/// Opens the latest release on GitHub (Android updates, "What's new").
#[tauri::command]
pub fn open_release_page(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(RELEASES_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_persist() {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("cfg/settings.json");
        assert_eq!(load(&p), Settings::default());
        assert!(Settings::default().check_updates);
        let s = Settings {
            check_updates: false,
            skipped: Some("1.0.1".into()),
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), s);
        std::fs::write(&p, b"{broken").unwrap();
        assert_eq!(load(&p), Settings::default());
        // Older files without a field get its default.
        std::fs::write(&p, br#"{"skipped":"1.0.2"}"#).unwrap();
        assert!(load(&p).check_updates);
    }
}
