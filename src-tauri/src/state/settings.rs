use serde::{Deserialize, Serialize};

/// Persisted application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Number of worker threads. None = auto (CPU count - 1).
    pub thread_count: Option<usize>,
    /// Custom log folder path. None = platform default.
    pub log_folder: Option<String>,
    /// Maximum retry attempts per file.
    pub max_retries: u32,
    /// Recently used folder paths.
    pub recent_folders: Vec<String>,
    /// Write EFC-format logs to disk after each run.
    #[serde(default)]
    pub verbose_logging: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            thread_count: None,
            max_retries: 3,
            log_folder: None,
            recent_folders: Vec::new(),
            verbose_logging: false,
        }
    }
}

/// Settings used for a specific processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingSettings {
    pub thread_count: usize,
    pub log_folder: String,
    pub max_retries: u32,
    pub verbose_logging: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_settings_default_thread_count_none() {
        let s = AppSettings::default();
        assert!(s.thread_count.is_none());
    }

    #[test]
    fn test_app_settings_default_max_retries_three() {
        let s = AppSettings::default();
        assert_eq!(s.max_retries, 3);
    }

    #[test]
    fn test_app_settings_default_log_folder_none() {
        let s = AppSettings::default();
        assert!(s.log_folder.is_none());
    }

    #[test]
    fn test_app_settings_default_recent_folders_empty() {
        let s = AppSettings::default();
        assert!(s.recent_folders.is_empty());
    }

    #[test]
    fn test_app_settings_serde_roundtrip_defaults() {
        let original = AppSettings::default();
        let json = serde_json::to_string(&original).expect("serialize");
        let back: AppSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(back.thread_count.is_none());
        assert_eq!(back.max_retries, 3);
        assert!(back.log_folder.is_none());
        assert!(back.recent_folders.is_empty());
    }

    #[test]
    fn test_app_settings_serde_roundtrip_custom() {
        let original = AppSettings {
            thread_count: Some(8),
            log_folder: Some("/var/log".to_string()),
            max_retries: 5,
            recent_folders: vec!["/music".to_string(), "/more".to_string()],
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: AppSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.thread_count, Some(8));
        assert_eq!(back.log_folder, Some("/var/log".to_string()));
        assert_eq!(back.max_retries, 5);
        assert_eq!(back.recent_folders, vec!["/music", "/more"]);
    }

    #[test]
    fn test_app_settings_json_uses_camel_case() {
        let s = AppSettings {
            thread_count: Some(4),
            log_folder: None,
            max_retries: 2,
            recent_folders: vec![],
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("threadCount"), "expected 'threadCount' in JSON: {json}");
        assert!(json.contains("maxRetries"), "expected 'maxRetries' in JSON: {json}");
        assert!(json.contains("recentFolders"), "expected 'recentFolders' in JSON: {json}");
    }
}
