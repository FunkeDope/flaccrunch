//! Menu-bar helpers: About info, Dev menu actions, opening folders and links.

use crate::state::app_state::AppState;
use crate::util::platform;
use std::path::PathBuf;
use tauri::State;

/// The only web pages the Help menu opens.
const PROJECT_URL: &str = "https://github.com/FunkeDope/flaccrunch";

/// libFLAC version string for Help → About.
#[tauri::command]
pub fn get_libflac_version() -> String {
    crate::flac::marker::libflac_version()
}

/// Open or close the web inspector (Dev → Toggle DevTools).
#[tauri::command]
pub fn toggle_devtools(window: tauri::WebviewWindow) {
    #[cfg(desktop)]
    {
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
    }
    #[cfg(not(desktop))]
    let _ = window;
}

/// What `open_external` may open.
fn resolve_target(target: &str, state: &AppState) -> Result<String, String> {
    match target {
        "settings-folder" => {
            let path = state
                .settings_path
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or("Settings location is unknown")?;
            let dir = path.parent().map(PathBuf::from).ok_or("No settings folder")?;
            ensure_dir(dir)
        }
        "log-folder" => {
            let custom = state
                .settings
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .log_folder
                .clone()
                .filter(|f| !f.trim().is_empty());
            let dir = custom
                .map(PathBuf::from)
                .unwrap_or_else(platform::default_log_folder);
            ensure_dir(dir)
        }
        "project" => Ok(PROJECT_URL.to_string()),
        "releases" => Ok(format!("{PROJECT_URL}/releases")),
        "issues" => Ok(format!("{PROJECT_URL}/issues")),
        other => Err(format!("Unknown target: {other}")),
    }
}

fn ensure_dir(dir: PathBuf) -> Result<String, String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir.to_string_lossy().to_string())
}

/// Open a known folder in the file manager or a project page in the browser.
/// Targets are names, not paths, so the page cannot open arbitrary locations.
#[tauri::command]
pub fn open_external(target: String, state: State<'_, AppState>) -> Result<(), String> {
    let resolved = resolve_target(&target, &state)?;
    open_with_system(&resolved)
}

#[cfg(desktop)]
fn open_with_system(target: &str) -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let mut cmd = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");
    cmd.arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {target}: {e}"))
}

#[cfg(not(desktop))]
fn open_with_system(_target: &str) -> Result<(), String> {
    Err("Not available on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_targets_are_rejected() {
        let state = AppState::with_startup_paths(Vec::new());
        assert!(resolve_target("C:\\Windows", &state).is_err());
        assert!(resolve_target("https://example.com", &state).is_err());
        assert_eq!(
            resolve_target("releases", &state).unwrap(),
            "https://github.com/FunkeDope/flaccrunch/releases"
        );
    }

    #[test]
    fn test_log_folder_uses_custom_folder_and_creates_it() {
        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("logs here");
        let state = AppState::with_startup_paths(Vec::new());
        state.settings.write().unwrap().log_folder = Some(custom.to_string_lossy().to_string());
        let resolved = resolve_target("log-folder", &state).unwrap();
        assert_eq!(PathBuf::from(resolved), custom);
        assert!(custom.is_dir());
    }
}
