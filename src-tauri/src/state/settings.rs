use serde::{Deserialize, Serialize};

/// Theme preference for the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

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
    /// UI theme preference.
    pub theme: Theme,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            thread_count: None,
            max_retries: 3,
            log_folder: None,
            recent_folders: Vec::new(),
            theme: Theme::default(),
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
}
