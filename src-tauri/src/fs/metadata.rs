use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Snapshot of a file's metadata for preservation across processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataSnapshot {
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    #[cfg(unix)]
    pub mode: Option<u32>,
}

/// Capture the metadata of a file for later restoration.
pub fn snapshot_metadata(path: &Path) -> io::Result<FileMetadataSnapshot> {
    let meta = std::fs::metadata(path)?;

    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let accessed = meta
        .accessed()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(FileMetadataSnapshot {
        modified,
        accessed,
        #[cfg(unix)]
        mode: {
            use std::os::unix::fs::PermissionsExt;
            Some(meta.permissions().mode())
        },
    })
}

/// Restore previously captured metadata to a file.
pub fn restore_metadata(path: &Path, snapshot: &FileMetadataSnapshot) -> io::Result<()> {
    // Restore modification time using filetime crate or std
    if let Some(mtime) = snapshot.modified {
        let mtime_system = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
        // Use the file's current accessed time if we don't have one
        let atime_system = if let Some(atime) = snapshot.accessed {
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(atime)
        } else {
            SystemTime::now()
        };

        // Set file times using platform-specific methods
        set_file_times(path, atime_system, mtime_system)?;
    }

    #[cfg(unix)]
    if let Some(mode) = snapshot.mode {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(path, perms)?;
    }

    Ok(())
}

#[cfg(unix)]
fn set_file_times(path: &Path, atime: SystemTime, mtime: SystemTime) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let atime_dur = atime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let mtime_dur = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let times = [
        libc::timespec {
            tv_sec: atime_dur.as_secs() as libc::time_t,
            tv_nsec: atime_dur.subsec_nanos() as libc::c_long,
        },
        libc::timespec {
            tv_sec: mtime_dur.as_secs() as libc::time_t,
            tv_nsec: mtime_dur.subsec_nanos() as libc::c_long,
        },
    ];

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let ret = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_file_times(_path: &Path, _atime: SystemTime, _mtime: SystemTime) -> io::Result<()> {
    // On non-unix, we'd use Windows-specific APIs
    // For now, this is a no-op placeholder
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_snapshot_metadata() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.flac");
        fs::write(&file_path, b"test data").unwrap();

        let snapshot = snapshot_metadata(&file_path).unwrap();
        assert!(snapshot.modified.is_some());
    }

    #[test]
    fn test_snapshot_nonexistent_file() {
        let result = snapshot_metadata(Path::new("/nonexistent/file.flac"));
        assert!(result.is_err());
    }

    #[test]
    fn test_snapshot_restore_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.flac");
        fs::write(&file_path, b"test data").unwrap();

        let snapshot = snapshot_metadata(&file_path).unwrap();

        // Modify the file to change timestamps
        std::thread::sleep(std::time::Duration::from_millis(100));
        fs::write(&file_path, b"modified data").unwrap();

        // Restore and verify
        restore_metadata(&file_path, &snapshot).unwrap();

        let restored = snapshot_metadata(&file_path).unwrap();
        assert_eq!(snapshot.modified, restored.modified);
    }
}
