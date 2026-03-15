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
