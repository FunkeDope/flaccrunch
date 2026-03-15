use super::run_state::RunState;
use super::settings::AppSettings;
use std::sync::{Arc, RwLock};

/// Global application state managed by Tauri.
pub struct AppState {
    pub active_run: RwLock<Option<Arc<RunState>>>,
    pub settings: RwLock<AppSettings>,
    /// Paths supplied on the command line (GUI mode): pre-populate the folder list.
    pub startup_paths: RwLock<Vec<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::with_startup_paths(Vec::new())
    }
}

impl AppState {
    pub fn with_startup_paths(paths: Vec<String>) -> Self {
        Self {
            active_run: RwLock::new(None),
            settings: RwLock::new(AppSettings::default()),
            startup_paths: RwLock::new(paths),
        }
    }
}
