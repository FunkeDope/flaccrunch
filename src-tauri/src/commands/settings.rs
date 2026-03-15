use crate::state::app_state::AppState;
use crate::state::settings::AppSettings;
use crate::util::platform;
use tauri::State;

/// Get the current application settings.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.read().unwrap().clone())
}

/// Save application settings.
#[tauri::command]
pub async fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut current = state.settings.write().unwrap();
    *current = settings;
    Ok(())
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
