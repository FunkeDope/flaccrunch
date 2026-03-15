use crate::fs::scanner;
use std::path::PathBuf;

/// Open a native file selection dialog for picking FLAC files.
/// Uses multi-select so users can Ctrl+click multiple files.
/// Works on both desktop and mobile (folder picking isn't supported on mobile).
#[tauri::command]
pub async fn select_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .add_filter("FLAC Audio", &["flac", "FLAC"])
        .pick_files(move |files| {
            let _ = tx.send(files);
        });

    let files = rx.await.map_err(|_| "Dialog cancelled".to_string())?;

    match files {
        Some(paths) => Ok(paths.iter().map(|p| p.to_string()).collect()),
        None => Ok(Vec::new()),
    }
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
