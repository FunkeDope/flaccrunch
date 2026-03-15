use std::path::PathBuf;

/// Get the number of logical CPU cores.
pub fn get_cpu_count() -> usize {
    num_cpus::get().max(1)
}

/// Get the default number of worker threads (CPU count - 1, minimum 1).
pub fn default_thread_count() -> usize {
    (get_cpu_count().saturating_sub(1)).max(1)
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

/// Set the process to below-normal priority.
#[cfg(target_os = "windows")]
pub fn set_process_below_normal(pid: u32) {
    use std::process::Command;
    // Use wmic to set below-normal priority (priority 6 = below normal)
    let _ = Command::new("wmic")
        .args(["process", "where", &format!("ProcessId={pid}"), "CALL", "setpriority", "16384"])
        .output();
}

#[cfg(unix)]
pub fn set_process_below_normal(pid: u32) {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, pid, 10);
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn set_process_below_normal(_pid: u32) {
    // No-op on unsupported platforms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cpu_count_positive() {
        assert!(get_cpu_count() >= 1);
    }

    #[test]
    fn test_default_thread_count_at_least_one() {
        assert!(default_thread_count() >= 1);
    }

    #[test]
    fn test_default_thread_count_less_than_cpu_count() {
        let cpus = get_cpu_count();
        let threads = default_thread_count();
        if cpus > 1 {
            assert_eq!(threads, cpus - 1);
        } else {
            assert_eq!(threads, 1);
        }
    }

    #[test]
    fn test_default_log_folder_not_empty() {
        let folder = default_log_folder();
        assert!(!folder.as_os_str().is_empty());
    }
}
