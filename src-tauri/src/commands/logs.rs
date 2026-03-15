use crate::state::app_state::AppState;
use tauri::State;

/// Get the run log contents.
#[tauri::command]
pub async fn get_run_log(
    _state: State<'_, AppState>,
) -> Result<String, String> {
    // TODO: Read from the active run's log file
    Ok(String::new())
}

/// Get the summary log contents.
#[tauri::command]
pub async fn get_summary_log(
    _state: State<'_, AppState>,
) -> Result<String, String> {
    // TODO: Generate from the active run's state
    Ok(String::new())
}

/// Open the log folder in the system file explorer.
#[tauri::command]
pub async fn open_log_folder(
    _state: State<'_, AppState>,
    _app: tauri::AppHandle,
) -> Result<(), String> {
    // TODO: Use tauri::api::shell::open to open the folder
    Ok(())
}
