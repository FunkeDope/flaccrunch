use crate::fs::scanner::{cleanup_stale_temps, scan_for_flac_files};
use crate::logging::run_log::RunLog;
use crate::pipeline::job::ProcessingContext;
use crate::pipeline::queue::JobQueue;
use crate::pipeline::worker_pool::run_worker_pool;
use crate::state::app_state::AppState;
use crate::state::run_state::{
    CompressionResult, FileEvent, ProcessingStatus, RunState, WorkerStatus,
};
use crate::state::settings::ProcessingSettings;
use crate::util::platform::default_log_folder;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// Start processing FLAC files in the given folders.
#[tauri::command]
pub async fn start_processing(
    folders: Vec<String>,
    settings: ProcessingSettings,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    // Check if already processing
    {
        let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref run) = *active {
            let status = run.status.read().unwrap_or_else(|e| e.into_inner());
            if *status == ProcessingStatus::Processing {
                return Err("Processing is already in progress".to_string());
            }
        }
    }

    let folder_paths: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();

    // Clean up stale temp files (only in directory paths, not individual files)
    let dir_paths: Vec<PathBuf> = folder_paths.iter().filter(|p| p.is_dir()).cloned().collect();
    cleanup_stale_temps(&dir_paths);

    // Scan for FLAC files
    let scan_result = scan_for_flac_files(&folder_paths);
    if scan_result.files.is_empty() {
        return Err("No FLAC files found in the selected folders".to_string());
    }

    let worker_count = settings.thread_count.min(scan_result.files.len());
    let run_id = uuid::Uuid::new_v4().to_string();

    // Set up logging
    let log_folder = if settings.log_folder.is_empty() {
        default_log_folder()
    } else {
        PathBuf::from(&settings.log_folder)
    };
    let run_log_dir = log_folder.join(format!(
        "run_{}",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));
    let _ = std::fs::create_dir_all(&run_log_dir);

    let run_log = RunLog::new(run_log_dir.join("run.log"))
        .map_err(|e| format!("Failed to create run log: {e}"))?;
    run_log.log(
        crate::logging::run_log::LogLevel::Info,
        &format!(
            "Starting FlacCrunch: {} files, {} workers",
            scan_result.files.len(),
            worker_count
        ),
    );

    // Create run state
    let run_state = Arc::new(RunState::new(run_id.clone(), worker_count));
    {
        let mut counters = run_state.counters.write().unwrap_or_else(|e| e.into_inner());
        counters.total_files = scan_result.files.len();
        counters.total_original_bytes = scan_result.total_size;
    }
    {
        let mut status = run_state.status.write().unwrap_or_else(|e| e.into_inner());
        *status = ProcessingStatus::Processing;
    }

    // Store run state
    {
        let mut active = state.active_run.write().unwrap_or_else(|e| e.into_inner());
        *active = Some(Arc::clone(&run_state));
    }

    // Create processing context
    let context = Arc::new(ProcessingContext {
        max_retries: settings.max_retries,
        scratch_dir: run_log_dir.join("scratch"),
    });
    let _ = std::fs::create_dir_all(&context.scratch_dir);

    // Create job queue
    let queue = Arc::new(JobQueue::new(scan_result.files));

    // Spawn the worker pool in a background task
    let run_state_clone = Arc::clone(&run_state);
    tokio::spawn(async move {
        run_worker_pool(worker_count, queue, context, run_state_clone, app).await;

        // Mark run as complete
        let mut status = run_state.status.write().unwrap_or_else(|e| e.into_inner());
        *status = ProcessingStatus::Complete;
    });

    Ok(run_id)
}

/// Cancel an active processing run.
#[tauri::command]
pub async fn cancel_processing(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    if let Some(ref run) = *active {
        {
            let mut status = run.status.write().unwrap_or_else(|e| e.into_inner());
            *status = ProcessingStatus::Cancelling;
        }
        run.cancel_token.cancel();
        Ok(())
    } else {
        Err("No active processing run".to_string())
    }
}

/// Get the current processing status.
#[tauri::command]
pub async fn get_processing_status(
    state: State<'_, AppState>,
) -> Result<ProcessingStatus, String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    if let Some(ref run) = *active {
        Ok(run.status.read().unwrap_or_else(|e| e.into_inner()).clone())
    } else {
        Ok(ProcessingStatus::Idle)
    }
}

/// Get the current status of all workers.
#[tauri::command]
pub async fn get_worker_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<WorkerStatus>, String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    if let Some(ref run) = *active {
        Ok(run.workers.read().unwrap_or_else(|e| e.into_inner()).clone())
    } else {
        Ok(Vec::new())
    }
}

/// Get recent file processing events.
#[tauri::command]
pub async fn get_recent_events(
    state: State<'_, AppState>,
) -> Result<Vec<FileEvent>, String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    if let Some(ref run) = *active {
        Ok(run.recent_events.read().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect())
    } else {
        Ok(Vec::new())
    }
}

/// Get top compression results.
#[tauri::command]
pub async fn get_top_compression(
    state: State<'_, AppState>,
) -> Result<Vec<CompressionResult>, String> {
    let active = state.active_run.read().unwrap_or_else(|e| e.into_inner());
    if let Some(ref run) = *active {
        Ok(run.top_compression.read().unwrap_or_else(|e| e.into_inner()).clone())
    } else {
        Ok(Vec::new())
    }
}
