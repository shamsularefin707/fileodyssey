use fo_core::registry::Registry;
use fo_core::runner::Runner;
use std::sync::Arc;

pub struct AppState {
    pub registry: Arc<Registry>,
    pub runner: Runner,
}

impl AppState {
    /// Real tools are registered here from M2 onwards.
    pub fn new() -> Self {
        let registry = Arc::new(Registry::new());
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(1)
            .max(1);
        let runner = Runner::new(Arc::clone(&registry), workers);
        Self { registry, runner }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
