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
/// On Android, files are always copied to the app cache so that processing
/// can read and write freely. The original content URI is stored for write-back
/// after processing (ACTION_OPEN_DOCUMENT gives read+write access to that URI).
fn resolve_filepath(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    fp: FilePath,
) -> Result<String, String> {
    // Desktop: FilePath is always a real filesystem path — use it directly.
    #[cfg(not(target_os = "android"))]
    if let Ok(path) = fp.clone().into_path() {
        return Ok(path.display().to_string());
    }

    // Android: always cache-copy regardless of whether the URI resolves to a real
    // path, because Android 11+ scoped storage prevents writing to arbitrary paths
    // on external storage without MANAGE_EXTERNAL_STORAGE.  Writing back through
    // the content URI (granted by ACTION_OPEN_DOCUMENT) is the correct approach.
    #[cfg(target_os = "android")]
    {
        use crate::state::app_state::AppState;
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

        // Derive the display filename.
        //
        // Priority order:
        //   1. ContentResolver.query(DISPLAY_NAME) — the real filename, same
        //      method every Android file manager uses.  Works for all SAF URIs
        //      (Downloads, MediaStore, ExternalStorage, etc.).
        //   2. Real path from FilePath::into_path() (rare on Android 11+).
        //   3. URI-derived fallback (msf-7247.flac style) — last resort.
        let filename = {
            use tauri::Manager;
            app.try_state::<crate::android_bridge::AndroidBridge>()
                .and_then(|bridge| bridge.get_display_name(&uri_str))
                .or_else(|| {
                    fp.clone().into_path().ok().and_then(|p| {
                        p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                    })
                })
                .unwrap_or_else(|| extract_filename_from_content_uri(&uri_str))
        };

        let dest = flac_cache.join(&filename);

        // Read via the fs plugin (handles both real paths and content:// URIs
        // through Android's ContentResolver).
        let content = app
            .fs()
            .read(fp)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        std::fs::write(&dest, &content)
            .map_err(|e| format!("Failed to write to cache: {e}"))?;

        // Record original identifier → cache path so write-back knows where to
        // push the compressed result.
        let cache_path = dest.display().to_string();
        let state = app.state::<AppState>();
        let mut map = state.content_uri_map.write().unwrap_or_else(|e| e.into_inner());
        map.insert(cache_path.clone(), uri_str);

        return Ok(cache_path);
    }

    #[cfg(not(target_os = "android"))]
    Err(format!("Cannot resolve file path: {}", fp))
}

/// Extract a human-readable filename from a content URI by URL-decoding it and
/// parsing common URI patterns.
///
/// This function is intentionally NOT gated by `cfg(target_os = "android")` so
/// it can be unit-tested on the host platform.
///
/// Examples:
/// - `content://com.android.providers.media.documents/document/audio%3A12345`
///   → `"audio-12345.flac"`   (opaque media-store ID, prefixed with type)
/// - `content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac`
///   → `"track.flac"`          (real filename inside the URI)
/// - `content://media/external/audio/media/12345`
///   → `"media-12345.flac"`   (fallback: authority slug + segment)
pub fn extract_filename_from_content_uri(uri_str: &str) -> String {
    let decoded = percent_decode(uri_str);

    // Take the last '/' segment of the decoded URI.
    let last = decoded.rsplit('/').next().unwrap_or("audio").to_string();

    // If the last segment already looks like a FLAC filename, use it as-is.
    if last.to_lowercase().ends_with(".flac") && !last.is_empty() {
        return last;
    }

    // Handle "type:value" patterns common in Android content URIs, e.g.:
    //   "audio:12345"              → opaque media-store ID
    //   "primary:Music/track.flac" → path embedded in a volume-prefixed segment
    if let Some(colon_pos) = last.find(':') {
        let type_part = &last[..colon_pos]; // e.g. "audio", "primary", "video"
        let value_part = &last[colon_pos + 1..]; // e.g. "12345", "Music/track.flac"

        // If the value is itself a path, pull the leaf filename.
        let leaf = value_part.rsplit('/').next().unwrap_or(value_part);
        if leaf.to_lowercase().ends_with(".flac") {
            return leaf.to_string();
        }

        // Opaque numeric/string ID: produce "audio-12345.flac" so it's
        // recognisable as an Android media-store entry rather than a real filename.
        // NOTE: to get the real display name here we would need a ContentResolver
        // query (OpenableColumns.DISPLAY_NAME) via JNI — that's a future improvement.
        let clean_value = value_part.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-");
        return format!("{type_part}-{clean_value}.flac");
    }

    // No colon — sanitise and append extension.
    let clean = last.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "-");
    if clean.to_lowercase().ends_with(".flac") {
        clean
    } else {
        format!("{clean}.flac")
    }
}

/// Simple percent-decoder (handles %XX sequences).
/// Not gated by `cfg(android)` so it can be unit-tested on any platform.
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── percent_decode ──────────────────────────────────────────────────────────

    #[test]
    fn test_percent_decode_plain() {
        assert_eq!(percent_decode("hello"), "hello");
    }

    #[test]
    fn test_percent_decode_colon_and_slash() {
        assert_eq!(percent_decode("primary%3AMusic%2Ftrack.flac"), "primary:Music/track.flac");
    }

    #[test]
    fn test_percent_decode_uppercase_hex() {
        assert_eq!(percent_decode("%2F"), "/");
        assert_eq!(percent_decode("%3A"), ":");
    }

    #[test]
    fn test_percent_decode_invalid_sequence_passthrough() {
        // %ZZ is not valid hex — passes through as-is
        assert_eq!(percent_decode("%ZZ"), "%ZZ");
    }

    // ── extract_filename_from_content_uri ───────────────────────────────────────

    #[test]
    fn test_extract_media_document_audio_id() {
        // content://com.android.providers.media.documents/document/audio%3A12345
        let uri = "content://com.android.providers.media.documents/document/audio%3A12345";
        let name = extract_filename_from_content_uri(uri);
        assert_eq!(name, "audio-12345.flac",
            "media-document opaque ID should be prefixed with type");
    }

    #[test]
    fn test_extract_external_storage_flac_path() {
        // content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac
        let uri = "content://com.android.externalstorage.documents/document/primary%3AMusic%2Ftrack.flac";
        let name = extract_filename_from_content_uri(uri);
        assert_eq!(name, "track.flac",
            "external storage URI with embedded real filename should extract the leaf name");
    }

    #[test]
    fn test_extract_plain_flac_segment() {
        // Last segment is already a .flac name
        let uri = "content://some.provider/files/my-album-track.flac";
        let name = extract_filename_from_content_uri(uri);
        assert_eq!(name, "my-album-track.flac");
    }

    #[test]
    fn test_extract_opaque_numeric_id_no_type() {
        // Plain numeric last segment with no colon — gets .flac appended
        let uri = "content://media/external/audio/media/99999";
        let name = extract_filename_from_content_uri(uri);
        assert_eq!(name, "99999.flac");
    }

    #[test]
    fn test_extract_returns_flac_extension_always() {
        let uri = "content://anything/document/somefile";
        let name = extract_filename_from_content_uri(uri);
        assert!(name.to_lowercase().ends_with(".flac"),
            "result must always end with .flac, got: {name}");
    }

    #[test]
    fn test_extract_empty_uri() {
        let name = extract_filename_from_content_uri("");
        assert!(name.to_lowercase().ends_with(".flac"));
    }

    #[test]
    fn test_percent_decode_full_uri() {
        let uri = "content://com.android.providers.media.documents/document/audio%3A12345";
        let decoded = percent_decode(uri);
        assert!(decoded.contains("audio:12345"));
    }
}
