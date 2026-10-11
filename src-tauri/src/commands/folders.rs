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
    app.dialog().file().pick_folders(move |folders| {
        let _ = tx.send(folders);
    });
    let result = rx.await.map_err(|_| "Dialog cancelled".to_string())?;

    match result {
        Some(paths) => paths
            .into_iter()
            .map(|p| resolve_filepath(&app, p))
            .collect(),
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
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        let bridge = app
            .try_state::<crate::android_bridge::AndroidBridge>()
            .ok_or_else(|| "AndroidBridge not available".to_string())?;

        let uris = bridge.select_input_files();
        let mut resolved = Vec::new();
        for uri in uris {
            let url = url::Url::parse(&uri)
                .map_err(|e| format!("Invalid Android document URI '{uri}': {e}"))?;
            resolved.push(resolve_filepath(&app, FilePath::Url(url))?);
        }
        return Ok(resolved);
    }

    #[cfg(not(target_os = "android"))]
    {
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
            Some(paths) => paths
                .into_iter()
                .map(|p| resolve_filepath(&app, p))
                .collect(),
            None => Ok(Vec::new()),
        }
    }
}

/// Convert a dialog FilePath to a usable filesystem path string.
/// On desktop, FilePath is always a Path variant and works directly.
/// On Android, files are always copied to the app cache so that processing
/// can read and write freely. The original content URI is stored for write-back
/// after processing (ACTION_OPEN_DOCUMENT gives read+write access to that URI).
fn resolve_filepath(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    fp: FilePath,
) -> Result<String, String> {
    #[cfg(not(target_os = "android"))]
    {
        fp.clone()
            .into_path()
            .map(|p| p.display().to_string())
            .map_err(|_| format!("Cannot resolve file path: {}", fp))
    }

    // Android: always cache-copy regardless of whether the URI resolves to a real
    // path, because Android 11+ scoped storage prevents writing to arbitrary paths
    // on external storage without MANAGE_EXTERNAL_STORAGE.  Writing back through
    // the content URI (granted by ACTION_OPEN_DOCUMENT) is the correct approach.
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

        // Display filename, in priority order:
        //   1. ContentResolver.query(DISPLAY_NAME) — the real filename.
        //   2. Real path from FilePath::into_path() (rare on Android 11+).
        //   3. URI-derived fallback (msf-7247.flac style) — last resort.
        let filename = app
            .try_state::<crate::android_bridge::AndroidBridge>()
            .and_then(|bridge| bridge.get_display_name(&uri_str))
            .or_else(|| {
                fp.clone().into_path().ok().and_then(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                })
            })
            .unwrap_or_else(|| extract_filename_from_content_uri(&uri_str));

        let dest = flac_cache.join(&filename);

        // Read via the fs plugin (handles both real paths and content:// URIs
        // through Android's ContentResolver).
        let content = app
            .fs()
            .read(fp)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        std::fs::write(&dest, &content).map_err(|e| format!("Failed to write to cache: {e}"))?;

        // Record original identifier → cache path so write-back knows where to
        // push the compressed result.
        let cache_path = dest.display().to_string();
        let state = app.state::<AppState>();
        state
            .content_uri_map
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cache_path.clone(), uri_str);

        Ok(cache_path)
    }
}

/// Extract a human-readable filename from a content URI by URL-decoding it and
/// parsing common URI patterns.
///
/// Not gated by `cfg(target_os = "android")` so it can be unit-tested on the
/// host platform.
///
/// Examples:
/// - `content://com.android.providers.media.documents/document/audio%3A12345`
///   → `"audio-12345.flac"`   (opaque media-store ID, prefixed with type)
/// - `content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac`
///   → `"track.flac"`          (real filename inside the URI)
/// - `content://media/external/audio/media/12345`
///   → `"12345.flac"`         (fallback: last segment + extension)
pub fn extract_filename_from_content_uri(uri_str: &str) -> String {
    let decoded = percent_decode(uri_str);

    let last = decoded.rsplit('/').next().unwrap_or("audio").to_string();
    if last.to_lowercase().ends_with(".flac") && !last.is_empty() {
        return last;
    }

    // "type:value" patterns common in Android content URIs, e.g.
    //   "audio:12345"              → opaque media-store ID
    //   "primary:Music/track.flac" → path embedded in a volume-prefixed segment
    if let Some(colon_pos) = last.find(':') {
        let type_part = &last[..colon_pos];
        let value_part = &last[colon_pos + 1..];
        let leaf = value_part.rsplit('/').next().unwrap_or(value_part);
        if leaf.to_lowercase().ends_with(".flac") {
            return leaf.to_string();
        }
        let clean_value =
            value_part.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-");
        return format!("{type_part}-{clean_value}.flac");
    }

    let clean = last.replace(
        |c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_',
        "-",
    );
    if clean.to_lowercase().ends_with(".flac") {
        clean
    } else {
        format!("{clean}.flac")
    }
}

/// Simple percent-decoder (handles %XX sequences).
pub fn percent_decode(s: &str) -> String {
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
    tokio::task::spawn_blocking(move || scanner::scan_for_flac_files(&paths))
        .await
        .map_err(|e| format!("Scan task panicked: {e}"))
}

/// Return any paths that were supplied on the command line so the frontend
/// can pre-populate the folder list. Consumed (cleared) after first call.
#[tauri::command]
pub async fn get_startup_paths(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut paths = state
        .startup_paths
        .write()
        .unwrap_or_else(|e| e.into_inner());
    Ok(std::mem::take(&mut *paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello"), "hello");
        assert_eq!(
            percent_decode("primary%3AMusic%2Ftrack.flac"),
            "primary:Music/track.flac"
        );
        assert_eq!(percent_decode("%2F"), "/");
        assert_eq!(percent_decode("%ZZ"), "%ZZ");
    }

    #[test]
    fn test_extract_media_document_audio_id() {
        let uri = "content://com.android.providers.media.documents/document/audio%3A12345";
        assert_eq!(extract_filename_from_content_uri(uri), "audio-12345.flac");
    }

    #[test]
    fn test_extract_external_storage_flac_path() {
        let uri =
            "content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac";
        assert_eq!(extract_filename_from_content_uri(uri), "track.flac");
    }

    #[test]
    fn test_extract_plain_flac_segment() {
        let uri = "content://some.provider/files/my-album-track.flac";
        assert_eq!(
            extract_filename_from_content_uri(uri),
            "my-album-track.flac"
        );
    }

    #[test]
    fn test_extract_opaque_numeric_id_no_type() {
        let uri = "content://media/external/audio/media/99999";
        assert_eq!(extract_filename_from_content_uri(uri), "99999.flac");
    }

    #[test]
    fn test_extract_always_has_flac_extension() {
        assert!(
            extract_filename_from_content_uri("content://anything/document/somefile")
                .ends_with(".flac")
        );
        assert!(extract_filename_from_content_uri("").ends_with(".flac"));
    }
}
