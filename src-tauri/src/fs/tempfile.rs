use std::path::{Path, PathBuf};
use std::{fs, io, thread, time::Duration};

/// Generate the temp file path for a FLAC file (.flac → .tmp).
pub fn temp_path_for(original: &Path) -> PathBuf {
    original.with_extension("tmp")
}

/// Generate the artwork temp file path (.flac → .arttmp).
pub fn art_temp_path(original: &Path) -> PathBuf {
    original.with_extension("arttmp")
}

/// Safely remove a file, ignoring errors if it doesn't exist.
pub fn safe_remove(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Safely move a file from `from` to `to`, retrying up to 5 times
/// with exponential backoff to handle transient file locks.
pub fn safe_move(from: &Path, to: &Path) -> io::Result<()> {
    let max_attempts = 5;
    let mut last_err = None;

    for attempt in 0..max_attempts {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt < max_attempts - 1 {
                    // Exponential backoff: 100ms, 200ms, 400ms, 800ms
                    let delay = Duration::from_millis(100 * (1 << attempt));
                    thread::sleep(delay);
                }
            }
        }
    }

    // If rename fails (e.g., cross-device), try copy + delete
    match fs::copy(from, to) {
        Ok(_) => {
            let _ = fs::remove_file(from);
            Ok(())
        }
        Err(_) => Err(last_err.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Failed to move file after retries")
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_temp_path_for() {
        let path = Path::new("/music/album/track.flac");
        assert_eq!(temp_path_for(path), PathBuf::from("/music/album/track.tmp"));
    }

    #[test]
    fn test_art_temp_path() {
        let path = Path::new("/music/track.flac");
        assert_eq!(
            art_temp_path(path),
            PathBuf::from("/music/track.arttmp")
        );
    }

    #[test]
    fn test_safe_remove_nonexistent() {
        // Should not panic
        safe_remove(Path::new("/nonexistent/file.tmp"));
    }

    #[test]
    fn test_safe_remove_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.tmp");
        fs::write(&path, b"data").unwrap();
        assert!(path.exists());
        safe_remove(&path);
        assert!(!path.exists());
    }

    #[test]
    fn test_safe_move_success() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("source.tmp");
        let dst = dir.path().join("dest.flac");
        fs::write(&src, b"flac data").unwrap();
        safe_move(&src, &dst).unwrap();
        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(fs::read(&dst).unwrap(), b"flac data");
    }
}
