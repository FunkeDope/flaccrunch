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
    /// Android only: SAF tree URI selected by the user for output (ACTION_OPEN_DOCUMENT_TREE).
    pub output_tree_uri: RwLock<Option<String>>,
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
            output_tree_uri: RwLock::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_no_active_run() {
        let state = AppState::default();
        let run = state.active_run.read().unwrap();
        assert!(run.is_none());
    }

    #[test]
    fn test_default_startup_paths_empty() {
        let state = AppState::default();
        let paths = state.startup_paths.read().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_with_startup_paths_stores_paths() {
        let paths = vec!["/music".to_string(), "/more".to_string()];
        let state = AppState::with_startup_paths(paths.clone());
        let stored = state.startup_paths.read().unwrap();
        assert_eq!(*stored, paths);
    }

    #[test]
    fn test_with_startup_paths_empty_vec() {
        let state = AppState::with_startup_paths(Vec::new());
        let paths = state.startup_paths.read().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_default_settings_match_app_settings_default() {
        let state = AppState::default();
        let settings = state.settings.read().unwrap();
        assert_eq!(settings.max_retries, 3);
        assert!(settings.thread_count.is_none());
        assert!(settings.log_folder.is_none());
        assert!(settings.recent_folders.is_empty());
    }

    #[test]
    fn test_content_uri_map_starts_empty() {
        let state = AppState::default();
        let map = state.content_uri_map.read().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_content_uri_map_can_be_written() {
        let state = AppState::default();
        {
            let mut map = state.content_uri_map.write().unwrap();
            map.insert("/cache/foo.flac".to_string(), "content://foo".to_string());
        }
        let map = state.content_uri_map.read().unwrap();
        assert_eq!(
            map.get("/cache/foo.flac").map(|s| s.as_str()),
            Some("content://foo")
        );
    }

    #[test]
    fn test_settings_can_be_overwritten() {
        let state = AppState::default();
        {
            let mut settings = state.settings.write().unwrap();
            settings.max_retries = 7;
        }
        let settings = state.settings.read().unwrap();
        assert_eq!(settings.max_retries, 7);
    }
}
