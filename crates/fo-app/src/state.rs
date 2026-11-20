use fo_core::registry::Registry;
use fo_core::runner::Runner;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub registry: Arc<Registry>,
    pub runner: Runner,
    pub placed: Placed,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        for tool in fo_core::image::tools::image_tools() {
            registry.register(tool);
        }
        for tool in fo_core::archive::tools::archive_tools() {
            registry.register(tool);
        }
        registry.register(fo_core::pdfopt::tool());
        registry.register(fo_core::office::tool());
        let registry = Arc::new(registry);
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(1)
            .max(1);
        let runner = Runner::new(Arc::clone(&registry), workers);
        Self {
            registry,
            runner,
            placed: Placed::default(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Results the runner produced in this session. Only these may be opened or
/// revealed from the UI, so the webview can't launch arbitrary programs.
#[derive(Default, Clone)]
pub struct Placed(Arc<Mutex<HashSet<String>>>);

impl Placed {
    pub fn record(&self, paths: &[String]) {
        let mut set = self.0.lock().expect("placed");
        set.extend(paths.iter().cloned());
    }

    pub fn contains(&self, path: &str) -> bool {
        self.0.lock().expect("placed").contains(path)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_state_registers_m3_tools() {
        let s = super::AppState::new();
        for id in [
            "arc.extract",
            "arc.create",
            "pdf.compress",
            "office.compress",
        ] {
            assert!(s.registry.get(id).is_some(), "{id} missing");
        }
    }

    #[test]
    fn app_state_registers_image_tools() {
        let s = super::AppState::new();
        for id in ["img.convert", "img.resize", "img.compress", "qr.read"] {
            assert!(s.registry.get(id).is_some(), "{id} missing");
        }
    }
}
