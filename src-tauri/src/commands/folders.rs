use crate::fs::scanner;
use std::path::PathBuf;
use tauri_plugin_dialog::FilePath;

/// Open a native folder selection dialog (desktop only).
/// Returns one or more folder paths for recursive FLAC scanning.
#[tauri::command]
#[cfg(desktop)]
pub async fn select_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .pick_folders(move |folders| {
            let _ = tx.send(folders);
        });

    let result = rx.await.map_err(|_| "Dialog cancelled".to_string())?;

    match result {
        Some(paths) => {
            let mut resolved = Vec::new();
            for p in paths {
                resolved.push(resolve_filepath(&app, p)?);
            }
            Ok(resolved)
        }
        None => Ok(Vec::new()),
    }
}

/// On mobile, folder picking is not supported — return an error.
#[tauri::command]
#[cfg(mobile)]
pub async fn select_folders(_app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Err("Folder picking is not supported on this platform. Use file selection instead.".to_string())
}

/// Open a native file selection dialog for picking individual FLAC files.
/// Works on all platforms. On Android, resolves content URIs by copying to cache.
#[tauri::command]
pub async fn select_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .add_filter("FLAC Audio", &["flac", "FLAC"])
        .pick_files(move |files| {
            let _ = tx.send(files);
        });

    let result = rx.await.map_err(|_| "Dialog cancelled".to_string())?;

    match result {
        Some(paths) => {
            let mut resolved = Vec::new();
            for p in paths {
                resolved.push(resolve_filepath(&app, p)?);
            }
            Ok(resolved)
        }
        None => Ok(Vec::new()),
    }
}

/// Convert a dialog FilePath to a usable filesystem path string.
/// On desktop, FilePath is always a Path variant and works directly.
/// On Android, FilePath may be a content:// URI that needs to be copied to cache.
fn resolve_filepath(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    fp: FilePath,
) -> Result<String, String> {
    // Try direct path conversion first (works on desktop, and for file:// URIs)
    if let Ok(path) = fp.clone().into_path() {
        return Ok(path.display().to_string());
    }

    // Content URI (Android) — read via fs plugin and copy to app cache
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        use tauri_plugin_fs::FsExt;

        let uri_str = fp.to_string();

        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Failed to get cache dir: {e}"))?;
        let flac_cache = cache_dir.join("flac_input");
        std::fs::create_dir_all(&flac_cache)
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;

        // Extract filename from URI or generate one
        let filename = uri_str
            .rsplit('/')
            .next()
            .unwrap_or("unknown.flac")
            .split('?')
            .next()
            .unwrap_or("unknown.flac");
        let filename = if filename.ends_with(".flac") || filename.ends_with(".FLAC") {
            filename.to_string()
        } else {
            format!("{filename}.flac")
        };
        let dest = flac_cache.join(&filename);

        // Read content URI via the fs plugin (handles Android content resolver)
        let content = app
            .fs()
            .read(fp)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        std::fs::write(&dest, &content)
            .map_err(|e| format!("Failed to write to cache: {e}"))?;

        return Ok(dest.display().to_string());
    }

    #[cfg(not(target_os = "android"))]
    Err(format!("Cannot resolve file path: {}", fp))
}

/// Return whether we're running on mobile (for UI to adapt).
#[tauri::command]
pub fn is_mobile() -> bool {
    cfg!(mobile)
}

/// Scan the given paths for FLAC files and return the result.
/// Paths can be directories (scanned recursively) or individual .flac files.
#[tauri::command]
pub async fn scan_folders(folders: Vec<String>) -> Result<scanner::ScanResult, String> {
    let paths: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
    Ok(scanner::scan_for_flac_files(&paths))
}

/// Validate that a folder path exists and is writable.
#[tauri::command]
pub async fn validate_folder(path: String) -> Result<bool, String> {
    match scanner::validate_folder(&PathBuf::from(&path).as_path()) {
        Ok(()) => Ok(true),
        Err(e) => Err(e),
    }
}
