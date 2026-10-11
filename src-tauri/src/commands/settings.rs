use crate::state::app_state::AppState;
use crate::state::settings::AppSettings;
use crate::util::platform;
use tauri::State;

/// Get the current application settings.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state
        .settings
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

/// Save application settings: in memory, and to the settings file so they
/// survive a restart.
#[tauri::command]
pub async fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.settings.write().unwrap_or_else(|e| e.into_inner()) = settings.clone();
    let path = state
        .settings_path
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(path) = path {
        settings.save(&path).map_err(|e| {
            format!(
                "Settings apply to this session, but could not be written to {}: {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

/// Where settings are persisted (for display in the Settings dialog).
#[tauri::command]
pub async fn get_settings_path(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state
        .settings_path
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default())
}

/// Get the number of CPU cores.
#[tauri::command]
pub async fn get_cpu_count() -> Result<usize, String> {
    Ok(platform::get_cpu_count())
}

/// Get the default log folder path.
#[tauri::command]
pub async fn get_default_log_folder() -> Result<String, String> {
    Ok(platform::default_log_folder().to_string_lossy().to_string())
}
