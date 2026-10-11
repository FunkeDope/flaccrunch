use serde::{Deserialize, Serialize};

/// Application settings (held in memory for the lifetime of the process).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Number of worker threads. None = auto (CPU count - 1).
    pub thread_count: Option<usize>,
    /// Custom log folder path. None = platform default.
    pub log_folder: Option<String>,
    /// Maximum retry attempts per file.
    pub max_retries: u32,
    /// Write EFC-format logs to disk after each run.
    #[serde(default)]
    pub verbose_logging: bool,
    /// Append a FLACCRUNCH_INFO Vorbis comment to re-encoded files.
    #[serde(default)]
    pub mark_as_crunched: bool,
    /// Skip files that already carry a FLACCRUNCH_INFO marker.
    #[serde(default)]
    pub skip_crunched: bool,
    /// Non-subset maximum compression (`--lax -l 32`, denser window search).
    /// Smaller files; some older hardware players cannot play non-subset FLAC.
    #[serde(default)]
    pub max_compression: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            thread_count: None,
            max_retries: 3,
            log_folder: None,
            verbose_logging: false,
            mark_as_crunched: false,
            skip_crunched: false,
            max_compression: false,
        }
    }
}

impl AppSettings {
    /// File name of the persisted settings inside the app config directory.
    pub const FILE_NAME: &'static str = "settings.json";

    /// Load settings from `path`. Missing file or unreadable JSON → `None`
    /// (callers fall back to defaults); unknown/missing fields are tolerated.
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Persist to `path` (creating parent directories), atomically via a
    /// temp file so a crash mid-write cannot leave a truncated file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
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
    #[serde(default)]
    pub mark_as_crunched: bool,
    #[serde(default)]
    pub skip_crunched: bool,
    #[serde(default)]
    pub max_compression: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_settings_defaults() {
        let s = AppSettings::default();
        assert!(s.thread_count.is_none());
        assert_eq!(s.max_retries, 3);
        assert!(s.log_folder.is_none());
        assert!(!s.verbose_logging);
        assert!(!s.mark_as_crunched);
        assert!(!s.skip_crunched);
    }

    #[test]
    fn test_app_settings_serde_roundtrip_custom() {
        let original = AppSettings {
            thread_count: Some(8),
            log_folder: Some("/var/log".to_string()),
            max_retries: 5,
            verbose_logging: true,
            mark_as_crunched: true,
            skip_crunched: false,
            max_compression: true,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("threadCount"), "{json}");
        assert!(json.contains("maxRetries"), "{json}");
        let back: AppSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.thread_count, Some(8));
        assert_eq!(back.log_folder, Some("/var/log".to_string()));
        assert_eq!(back.max_retries, 5);
        assert!(back.verbose_logging);
        assert!(back.mark_as_crunched);
    }

    #[test]
    fn test_settings_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(AppSettings::FILE_NAME);
        assert!(AppSettings::load(&path).is_none(), "missing file → None");

        let s = AppSettings {
            thread_count: Some(3),
            max_compression: true,
            mark_as_crunched: true,
            ..AppSettings::default()
        };
        s.save(&path).unwrap();
        let back = AppSettings::load(&path).unwrap();
        assert_eq!(back.thread_count, Some(3));
        assert!(back.max_compression);
        assert!(back.mark_as_crunched);
        assert!(!path.with_extension("json.tmp").exists());

        std::fs::write(&path, "{ not json").unwrap();
        assert!(AppSettings::load(&path).is_none(), "corrupt file → None");
    }

    #[test]
    fn test_app_settings_ignores_unknown_fields_from_older_frontends() {
        let json = r#"{"threadCount":null,"logFolder":null,"maxRetries":2,"recentFolders":["/x"]}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.max_retries, 2);
        assert!(!s.verbose_logging);
    }
}
