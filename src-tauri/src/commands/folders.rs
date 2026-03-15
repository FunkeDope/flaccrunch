use crate::fs::scanner;
use std::path::PathBuf;

/// Open a native folder selection dialog.
#[tauri::command]
pub async fn select_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });

    let folder = rx.await.map_err(|_| "Dialog cancelled".to_string())?;

    match folder {
        Some(path) => Ok(vec![path.to_string()]),
        None => Ok(Vec::new()),
    }
}

/// Scan the given folders for FLAC files and return the result.
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
