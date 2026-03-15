use crate::fs::scanner;
use crate::state::app_state::AppState;
use std::path::PathBuf;
use tauri::State;
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
/// On Android, FilePath may be a content:// URI that needs to be resolved.
fn resolve_filepath(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    fp: FilePath,
) -> Result<String, String> {
    // Try direct path conversion first (works on desktop, and for file:// URIs)
    if let Ok(path) = fp.clone().into_path() {
        return Ok(path.display().to_string());
    }

    // Content URI (Android)
    #[cfg(target_os = "android")]
    {
        use crate::state::app_state::AppState;
        use tauri::Manager;
        use tauri_plugin_fs::FsExt;

        let uri_str = fp.to_string();

        // For external storage document URIs we can resolve to a real path and
        // work on the file directly (no cache copy, no write-back needed).
        if let Some(real_path) = resolve_external_storage_uri(&uri_str) {
            return Ok(real_path);
        }

        // Fall back: copy to app cache and remember the original URI for write-back.
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Failed to get cache dir: {e}"))?;
        let flac_cache = cache_dir.join("flac_input");
        std::fs::create_dir_all(&flac_cache)
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;

        let filename = extract_filename_from_content_uri(&uri_str);
        let dest = flac_cache.join(&filename);

        let content = app
            .fs()
            .read(fp)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        std::fs::write(&dest, &content)
            .map_err(|e| format!("Failed to write to cache: {e}"))?;

        // Store the mapping for write-back after processing.
        let cache_path = dest.display().to_string();
        let state = app.state::<AppState>();
        let mut map = state.content_uri_map.write().unwrap_or_else(|e| e.into_inner());
        map.insert(cache_path.clone(), uri_str);

        return Ok(cache_path);
    }

    #[cfg(not(target_os = "android"))]
    Err(format!("Cannot resolve file path: {}", fp))
}

/// For `content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac`
/// resolve to the real filesystem path `/storage/emulated/0/Music/track.flac`.
/// Returns `None` for any other URI authority (media provider, downloads, etc.).
#[cfg(target_os = "android")]
fn resolve_external_storage_uri(uri_str: &str) -> Option<String> {
    if !uri_str.contains("com.android.externalstorage.documents/document/") {
        return None;
    }
    let encoded = uri_str.split("/document/").nth(1)?;
    let doc_id = percent_decode(encoded); // e.g. "primary:Music/track.flac"
    let colon = doc_id.find(':')?;
    let volume = &doc_id[..colon];
    let rel = &doc_id[colon + 1..];
    let base = if volume.eq_ignore_ascii_case("primary") {
        "/storage/emulated/0".to_string()
    } else {
        format!("/storage/{}", volume)
    };
    let real_path = format!("{}/{}", base, rel);
    if std::path::Path::new(&real_path).exists() {
        Some(real_path)
    } else {
        None
    }
}

/// Extract a human-readable filename from a content URI by URL-decoding it and
/// parsing common URI patterns.
#[cfg(target_os = "android")]
fn extract_filename_from_content_uri(uri_str: &str) -> String {
    let decoded = percent_decode(uri_str);
    // Last path segment after final '/'
    let last = decoded.rsplit('/').next().unwrap_or("audio");
    // Strip a leading "primary:" volume prefix that can appear in external storage URIs
    let name = if let Some(after_colon) = last.strip_prefix(|c: char| c.is_ascii_alphabetic())
        .and_then(|_| last.find(':'))
        .map(|i| &last[i + 1..])
    {
        // e.g. "primary:track.flac" → "track.flac", or just use what's after the colon
        after_colon.rsplit('/').next().unwrap_or(after_colon)
    } else {
        last
    };
    // Ensure it looks like a FLAC filename
    if name.to_lowercase().ends_with(".flac") && !name.is_empty() {
        name.to_string()
    } else {
        // For opaque IDs like "audio:12345" → "audio-12345.flac"
        let clean = name.replace(':', "-");
        format!("{clean}.flac")
    }
}

/// Simple percent-decoder (handles %XX sequences).
#[cfg(target_os = "android")]
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(hi), Some(lo)) = (hex_nibble(b[i + 1]), hex_nibble(b[i + 2])) {
                out.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

#[cfg(target_os = "android")]
fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
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
    match scanner::validate_folder(PathBuf::from(&path).as_path()) {
        Ok(()) => Ok(true),
        Err(e) => Err(e),
    }
}

/// Return any paths that were supplied on the command line so the frontend
/// can pre-populate the folder list. Consumed (cleared) after first call.
#[tauri::command]
pub async fn get_startup_paths(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut paths = state.startup_paths.write().unwrap_or_else(|e| e.into_inner());
    Ok(std::mem::take(&mut *paths))
}
