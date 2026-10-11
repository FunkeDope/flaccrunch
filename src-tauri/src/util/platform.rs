use std::path::PathBuf;

/// Get the number of logical CPU cores.
pub fn get_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1)
}

/// Get the default number of worker threads (CPU count - 1, minimum 1).
pub fn default_thread_count() -> usize {
    (get_cpu_count().saturating_sub(1)).max(1)
}

/// libFLAC threads to give each worker so that `workers × threads` fills
/// the machine: 1 when there are at least as many workers as cores.
pub fn encoder_threads_for(worker_count: usize) -> u32 {
    (get_cpu_count() / worker_count.max(1)).clamp(1, 128) as u32
}

/// Get the platform-specific default log folder.
pub fn default_log_folder() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(desktop) = dirs.desktop_dir() {
            return desktop.join("EFC-logs");
        }
    }
    if let Some(dirs) = directories::BaseDirs::new() {
        return dirs.data_local_dir().join("FlacCrunch").join("logs");
    }
    PathBuf::from("EFC-logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cpu_count_positive() {
        assert!(get_cpu_count() >= 1);
    }

    #[test]
    fn test_default_thread_count_less_than_cpu_count() {
        let cpus = get_cpu_count();
        let threads = default_thread_count();
        assert!(threads >= 1);
        if cpus > 1 {
            assert_eq!(threads, cpus - 1);
        } else {
            assert_eq!(threads, 1);
        }
    }

    #[test]
    fn test_encoder_threads_for() {
        let cpus = get_cpu_count();
        assert_eq!(encoder_threads_for(cpus), 1);
        assert_eq!(encoder_threads_for(cpus * 2), 1);
        assert_eq!(encoder_threads_for(1), cpus.min(128) as u32);
        assert_eq!(encoder_threads_for(0), cpus.min(128) as u32);
    }

    #[test]
    fn test_default_log_folder_not_empty() {
        assert!(!default_log_folder().as_os_str().is_empty());
    }
}
