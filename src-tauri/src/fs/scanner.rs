use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A scanned FLAC file with its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

/// Result of scanning folders for FLAC files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    pub permission_errors: Vec<String>,
    pub total_size: u64,
}

/// Recursively scan the given folders for .flac files.
/// Returns files sorted by size descending (largest first), then by name ascending.
pub fn scan_for_flac_files(folders: &[PathBuf]) -> ScanResult {
    let mut files = Vec::new();
    let mut permission_errors = Vec::new();
    let mut total_size: u64 = 0;

    for folder in folders {
        for entry in WalkDir::new(folder).follow_links(true) {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_file() {
                        if let Some(ext) = entry.path().extension() {
                            if ext.eq_ignore_ascii_case("flac") {
                                if let Ok(meta) = entry.metadata() {
                                    let size = meta.len();
                                    total_size += size;
                                    files.push(ScannedFile {
                                        path: entry.path().to_path_buf(),
                                        name: entry
                                            .file_name()
                                            .to_string_lossy()
                                            .to_string(),
                                        size,
                                    });
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    permission_errors.push(format!("{}", e));
                }
            }
        }
    }

    // Sort: largest first, then alphabetically for same size
    files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));

    ScanResult {
        files,
        permission_errors,
        total_size,
    }
}

/// Remove stale .tmp files where a matching .flac file exists.
pub fn cleanup_stale_temps(folders: &[PathBuf]) {
    for folder in folders {
        for entry in WalkDir::new(folder).follow_links(true) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext.eq_ignore_ascii_case("tmp") {
                            let flac_path = path.with_extension("flac");
                            if flac_path.exists() {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check if a directory exists and is writable.
pub fn validate_folder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }
    // Try to check write access by creating a temp file
    let test_file = path.join(".flaccrunch_write_test");
    match std::fs::File::create(&test_file) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            Ok(())
        }
        Err(e) => Err(format!(
            "No write access to {}: {}",
            path.display(),
            e
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    #[test]
    fn test_scan_empty_directory() {
        let dir = create_test_dir();
        let result = scan_for_flac_files(&[dir.path().to_path_buf()]);
        assert!(result.files.is_empty());
        assert_eq!(result.total_size, 0);
    }

    #[test]
    fn test_scan_finds_flac_files() {
        let dir = create_test_dir();
        fs::write(dir.path().join("test.flac"), b"fake flac data").unwrap();
        fs::write(dir.path().join("test2.FLAC"), b"fake flac data 2").unwrap();
        let result = scan_for_flac_files(&[dir.path().to_path_buf()]);
        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn test_scan_ignores_non_flac() {
        let dir = create_test_dir();
        fs::write(dir.path().join("test.mp3"), b"not flac").unwrap();
        fs::write(dir.path().join("test.wav"), b"not flac either").unwrap();
        fs::write(dir.path().join("test.flac"), b"is flac").unwrap();
        let result = scan_for_flac_files(&[dir.path().to_path_buf()]);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "test.flac");
    }

    #[test]
    fn test_scan_sorts_by_size_desc() {
        let dir = create_test_dir();
        fs::write(dir.path().join("small.flac"), b"sm").unwrap();
        fs::write(dir.path().join("large.flac"), b"large content here!").unwrap();
        fs::write(dir.path().join("medium.flac"), b"medium data").unwrap();
        let result = scan_for_flac_files(&[dir.path().to_path_buf()]);
        assert_eq!(result.files.len(), 3);
        assert!(result.files[0].size >= result.files[1].size);
        assert!(result.files[1].size >= result.files[2].size);
    }

    #[test]
    fn test_scan_multiple_folders() {
        let dir1 = create_test_dir();
        let dir2 = create_test_dir();
        fs::write(dir1.path().join("a.flac"), b"data1").unwrap();
        fs::write(dir2.path().join("b.flac"), b"data2").unwrap();
        let result =
            scan_for_flac_files(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn test_scan_recursive() {
        let dir = create_test_dir();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.flac"), b"nested data").unwrap();
        let result = scan_for_flac_files(&[dir.path().to_path_buf()]);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "nested.flac");
    }

    #[test]
    fn test_cleanup_stale_temps() {
        let dir = create_test_dir();
        fs::write(dir.path().join("track.flac"), b"flac data").unwrap();
        fs::write(dir.path().join("track.tmp"), b"temp data").unwrap();
        fs::write(dir.path().join("orphan.tmp"), b"no matching flac").unwrap();
        cleanup_stale_temps(&[dir.path().to_path_buf()]);
        // track.tmp should be removed (matching .flac exists)
        assert!(!dir.path().join("track.tmp").exists());
        // orphan.tmp should remain (no matching .flac)
        assert!(dir.path().join("orphan.tmp").exists());
    }

    #[test]
    fn test_validate_folder_exists() {
        let dir = create_test_dir();
        assert!(validate_folder(dir.path()).is_ok());
    }

    #[test]
    fn test_validate_folder_nonexistent() {
        let result = validate_folder(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
