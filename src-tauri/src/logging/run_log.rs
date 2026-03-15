use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Log level for run log entries.
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Success,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Success => write!(f, "OK"),
        }
    }
}

/// Buffered run log that writes to a file.
pub struct RunLog {
    path: PathBuf,
    buffer: Mutex<Vec<String>>,
}

impl RunLog {
    /// Create a new run log at the specified path.
    pub fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Create/truncate the file
        File::create(&path)?;
        Ok(Self {
            path,
            buffer: Mutex::new(Vec::with_capacity(40)),
        })
    }

    /// Add a log entry.
    pub fn log(&self, level: LogLevel, message: &str) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let entry = format!("[{timestamp}] [{level}] {message}");

        let mut buffer = self.buffer.lock().unwrap();
        buffer.push(entry);

        // Auto-flush every 40 lines
        if buffer.len() >= 40 {
            self.flush_buffer(&mut buffer);
        }
    }

    /// Flush the buffer to disk.
    pub fn flush(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        self.flush_buffer(&mut buffer);
    }

    fn flush_buffer(&self, buffer: &mut Vec<String>) {
        if buffer.is_empty() {
            return;
        }
        if let Ok(mut file) = OpenOptions::new().append(true).open(&self.path) {
            for line in buffer.iter() {
                let _ = writeln!(file, "{}", line);
            }
        }
        buffer.clear();
    }

    /// Get the log file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the entire log file contents.
    pub fn read_contents(&self) -> String {
        self.flush();
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

impl Drop for RunLog {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    // --- LogLevel Display ---

    #[test]
    fn test_log_level_display_info() {
        assert_eq!(format!("{}", LogLevel::Info), "INFO");
    }

    #[test]
    fn test_log_level_display_warn() {
        assert_eq!(format!("{}", LogLevel::Warn), "WARN");
    }

    #[test]
    fn test_log_level_display_error() {
        assert_eq!(format!("{}", LogLevel::Error), "ERROR");
    }

    #[test]
    fn test_log_level_display_success() {
        assert_eq!(format!("{}", LogLevel::Success), "OK");
    }

    // --- RunLog::new ---

    #[test]
    fn test_run_log_new_creates_file() {
        let path = temp_log_path("flaccrunch_test_run_log_new.log");
        let log = RunLog::new(path.clone()).expect("RunLog::new should succeed");
        assert!(path.exists(), "log file should be created on disk");
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_run_log_new_returns_correct_path() {
        let path = temp_log_path("flaccrunch_test_run_log_path.log");
        let log = RunLog::new(path.clone()).expect("RunLog::new should succeed");
        assert_eq!(log.path(), path.as_path());
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    // --- RunLog::log + read_contents ---

    #[test]
    fn test_log_and_read_contents_includes_message() {
        let path = temp_log_path("flaccrunch_test_run_log_msg.log");
        let log = RunLog::new(path.clone()).expect("create log");
        log.log(LogLevel::Info, "hello world");
        let contents = log.read_contents();
        assert!(
            contents.contains("hello world"),
            "log contents should include logged message, got: {contents}"
        );
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_log_entries_include_level_label() {
        let path = temp_log_path("flaccrunch_test_run_log_level.log");
        let log = RunLog::new(path.clone()).expect("create log");
        log.log(LogLevel::Error, "something failed");
        let contents = log.read_contents();
        assert!(
            contents.contains("[ERROR]"),
            "log should contain level label [ERROR], got: {contents}"
        );
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_multiple_log_entries_all_present() {
        let path = temp_log_path("flaccrunch_test_run_log_multi.log");
        let log = RunLog::new(path.clone()).expect("create log");
        log.log(LogLevel::Info, "first entry");
        log.log(LogLevel::Warn, "second entry");
        log.log(LogLevel::Success, "third entry");
        let contents = log.read_contents();
        assert!(contents.contains("first entry"), "must contain first");
        assert!(contents.contains("second entry"), "must contain second");
        assert!(contents.contains("third entry"), "must contain third");
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_flush_writes_to_disk() {
        let path = temp_log_path("flaccrunch_test_run_log_flush.log");
        let log = RunLog::new(path.clone()).expect("create log");
        log.log(LogLevel::Info, "flush test");
        log.flush();
        // Read directly from disk (bypassing RunLog)
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            contents.contains("flush test"),
            "flushed content should be on disk, got: {contents}"
        );
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_auto_flush_at_40_entries() {
        let path = temp_log_path("flaccrunch_test_run_log_autoflush.log");
        let log = RunLog::new(path.clone()).expect("create log");
        for i in 0..40 {
            log.log(LogLevel::Info, &format!("line {i}"));
        }
        // After 40 lines the auto-flush should have fired; read directly from disk
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            contents.contains("line 0"),
            "auto-flush should have written to disk by line 40, got: {contents}"
        );
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_log_entries_contain_timestamp_brackets() {
        let path = temp_log_path("flaccrunch_test_run_log_ts.log");
        let log = RunLog::new(path.clone()).expect("create log");
        log.log(LogLevel::Info, "ts check");
        let contents = log.read_contents();
        assert!(
            contents.contains('['),
            "log entries should contain a bracketed timestamp"
        );
        drop(log);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_new_log_creates_parent_dirs() {
        let base = std::env::temp_dir()
            .join("flaccrunch_test_nested_dirs")
            .join("subdir");
        let path = base.join("run.log");
        let _ = std::fs::remove_dir_all(&base); // ensure clean start
        let log = RunLog::new(path.clone()).expect("should create parent dirs and log file");
        assert!(path.exists());
        drop(log);
        let _ = std::fs::remove_dir_all(&base);
    }
}
