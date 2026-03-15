use std::path::PathBuf;
use tauri::Manager;

/// Resolve the path to the bundled `flac` sidecar binary.
/// In development mode, falls back to system PATH.
pub fn resolve_flac(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_sidecar(app_handle, "binaries/flac", "flac")
}

/// Resolve the path to the bundled `metaflac` sidecar binary.
pub fn resolve_metaflac(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_sidecar(app_handle, "binaries/metaflac", "metaflac")
}

fn resolve_sidecar(
    app_handle: &tauri::AppHandle,
    sidecar_name: &str,
    fallback_name: &str,
) -> Result<PathBuf, String> {
    // Try Tauri's resource resolver first
    if let Ok(resource_path) = app_handle
        .path()
        .resolve(sidecar_name, tauri::path::BaseDirectory::Resource)
    {
        if resource_path.exists() {
            return Ok(resource_path);
        }
    }

    // Fallback: try to find in system PATH
    if let Ok(path) = which::which(fallback_name) {
        return Ok(path);
    }

    Err(format!(
        "Could not find '{}' binary. Ensure it is bundled or available in PATH.",
        fallback_name
    ))
}
