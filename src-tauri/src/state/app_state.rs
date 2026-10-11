use super::run_state::RunState;
use super::settings::AppSettings;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Global application state managed by Tauri.
pub struct AppState {
    pub active_run: RwLock<Option<Arc<RunState>>>,
    pub settings: RwLock<AppSettings>,
    /// Paths supplied on the command line (GUI mode): pre-populate the folder list.
    pub startup_paths: RwLock<Vec<String>>,
    /// Android only: maps cache file path → original content URI for write-back after processing.
    pub content_uri_map: RwLock<HashMap<String, String>>,
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
            content_uri_map: RwLock::new(HashMap::new()),
        }
    }

    /// The current run, if any.
    pub fn active_run(&self) -> Option<Arc<RunState>> {
        self.active_run
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = AppState::default();
        assert!(state.active_run().is_none());
        assert!(state.startup_paths.read().unwrap().is_empty());
        assert!(state.content_uri_map.read().unwrap().is_empty());
        assert_eq!(state.settings.read().unwrap().max_retries, 3);
    }

    #[test]
    fn test_with_startup_paths_stores_paths() {
        let paths = vec!["/music".to_string(), "/more".to_string()];
        let state = AppState::with_startup_paths(paths.clone());
        assert_eq!(*state.startup_paths.read().unwrap(), paths);
    }

    #[test]
    fn test_settings_can_be_overwritten() {
        let state = AppState::default();
        state.settings.write().unwrap().max_retries = 7;
        assert_eq!(state.settings.read().unwrap().max_retries, 7);
    }
}
