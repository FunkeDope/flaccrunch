use crate::logging::efc_log::generate_efc_log;
use crate::state::app_state::AppState;
use crate::state::run_state::{FileEvent, RunSummary};
use tauri::State;

/// Generate an EFC-format log string from the current run.
///
/// The frontend passes its full in-memory event list, timing, and run metadata;
/// the backend contributes counters and top-compression from the run state.
#[tauri::command]
pub async fn get_efc_log(
    events: Vec<FileEvent>,
    elapsed_secs: u64,
    source_folder: String,
    start_ms: i64,
    finish_ms: i64,
    thread_count: usize,
    max_retries: u32,
    run_canceled: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    let (counters, top_compression) = if let Some(ref run) = *active {
        let c = run.counters.read().unwrap_or_else(|e| e.into_inner()).clone();
        let t = run.top_compression.read().unwrap_or_else(|e| e.into_inner()).clone();
        (c, t)
    } else {
        return Err("No active run".to_string());
    };

    let summary = RunSummary {
        counters,
        elapsed_secs,
        top_compression,
        status_lines: vec![],
        source_folder,
        start_ms,
        finish_ms,
        thread_count,
        max_retries,
        run_canceled,
    };

    Ok(generate_efc_log(&summary, &events))
}

/// Write text content to an arbitrary path (bypasses WebView FS scope restrictions).
/// Used by the frontend to save export logs to user-chosen paths via the native save dialog.
#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content.as_bytes())
        .map_err(|e| format!("Failed to write file: {e}"))
}
