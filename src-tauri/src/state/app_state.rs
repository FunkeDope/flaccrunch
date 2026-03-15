use super::run_state::RunState;
use super::settings::AppSettings;
use std::sync::{Arc, RwLock};

/// Global application state managed by Tauri.
pub struct AppState {
    pub active_run: RwLock<Option<Arc<RunState>>>,
    pub settings: RwLock<AppSettings>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_run: RwLock::new(None),
            settings: RwLock::new(AppSettings::default()),
        }
    }
}
