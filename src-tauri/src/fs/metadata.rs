use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Snapshot of a file's metadata for preservation across processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataSnapshot {
    pub created: Option<u64>,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    #[cfg(unix)]
    pub mode: Option<u32>,
    #[cfg(windows)]
    pub attributes: Option<u32>,
}

/// Capture the metadata of a file for later restoration.
pub fn snapshot_metadata(path: &Path) -> io::Result<FileMetadataSnapshot> {
    let meta = std::fs::metadata(path)?;

    let created = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

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
        created,
        modified,
        accessed,
        #[cfg(unix)]
        mode: {
            use std::os::unix::fs::PermissionsExt;
            Some(meta.permissions().mode())
        },
        #[cfg(windows)]
        attributes: {
            use std::os::windows::fs::MetadataExt;
            Some(meta.file_attributes())
        },
    })
}

/// Restore previously captured metadata to a file.
pub fn restore_metadata(path: &Path, snapshot: &FileMetadataSnapshot) -> io::Result<()> {
    set_file_times(path, snapshot)?;

    #[cfg(unix)]
    if let Some(mode) = snapshot.mode {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(path, perms)?;
    }

    #[cfg(windows)]
    if let Some(attrs) = snapshot.attributes {
        set_file_attributes_windows(path, attrs)?;
    }

    Ok(())
}

fn set_file_times(path: &Path, snapshot: &FileMetadataSnapshot) -> io::Result<()> {
    use std::fs::{FileTimes, OpenOptions};

    let Some(mtime) = snapshot.modified else {
        return Ok(());
    };

    let mtime_system = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
    let atime_system = snapshot
        .accessed
        .map(|a| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(a))
        .unwrap_or(mtime_system);

    let mut times = FileTimes::new()
        .set_accessed(atime_system)
        .set_modified(mtime_system);

    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTimesExt;
        if let Some(ctime) = snapshot.created {
            let ctime_system =
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(ctime);
            times = times.set_created(ctime_system);
        }
    }

    let file = OpenOptions::new().write(true).open(path)?;
    file.set_times(times)?;

    Ok(())
}

/// Restore Windows file attributes (hidden, system, read-only, etc.) using
/// the Win32 SetFileAttributesW API. This is a no-op on non-Windows targets.
#[cfg(windows)]
fn set_file_attributes_windows(path: &Path, attributes: u32) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn SetFileAttributesW(lp_file_name: *const u16, dw_file_attributes: u32) -> i32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe { SetFileAttributesW(wide.as_ptr(), attributes) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
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
