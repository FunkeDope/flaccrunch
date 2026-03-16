use crate::fs::scanner::{cleanup_stale_temps, scan_for_flac_files};
use crate::logging::efc_log::generate_efc_log;
use crate::pipeline::job::ProcessingContext;
use crate::pipeline::queue::JobQueue;
use crate::pipeline::worker_pool::run_worker_pool;
use crate::state::app_state::AppState;
use crate::state::run_state::{
    CompressionResult, FileEvent, ProcessingStatus, RunState, RunSummary, WorkerStatus,
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

    // Clean up stale temp files
    let dir_paths: Vec<PathBuf> = folder_paths.iter().filter(|p| p.is_dir()).cloned().collect();
    cleanup_stale_temps(&dir_paths);

    // Scan for FLAC files
    let scan_result = scan_for_flac_files(&folder_paths);
    if scan_result.files.is_empty() {
        return Err("No FLAC files found in the selected folders".to_string());
    }

    let worker_count = settings.thread_count.min(scan_result.files.len());
    let run_id = uuid::Uuid::new_v4().to_string();

    // Scratch dir lives in the system temp directory — no persistent log folder needed
    let scratch_dir = std::env::temp_dir()
        .join("flaccrunch_scratch")
        .join(&run_id);
    let _ = std::fs::create_dir_all(&scratch_dir);

    // Resolve the verbose-log output folder (only used when verbose_logging is true)
    let verbose_log_dir: Option<PathBuf> = if settings.verbose_logging {
        let base = if settings.log_folder.is_empty() {
            #[cfg(mobile)]
            {
                use tauri::Manager;
                app.path().app_cache_dir().unwrap_or_else(|_| default_log_folder())
            }
            #[cfg(not(mobile))]
            { default_log_folder() }
        } else {
            PathBuf::from(&settings.log_folder)
        };
        let dir = base.join(format!(
            "run_{}",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        ));
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    } else {
        None
    };

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

    let context = Arc::new(ProcessingContext {
        max_retries: settings.max_retries,
        scratch_dir,
    });

    let queue = Arc::new(JobQueue::new(scan_result.files));
    let run_state_clone = Arc::clone(&run_state);
    let source_folder_str = folders.join(", ");
    let thread_count = worker_count;
    let max_retries = settings.max_retries;
    let start_ms = chrono::Local::now().timestamp_millis();

    tokio::spawn(async move {
        run_worker_pool(worker_count, queue, context, Arc::clone(&run_state_clone), app).await;

        // Write EFC log to disk if verbose logging is enabled
        if let Some(log_dir) = verbose_log_dir {
            let finish_ms = chrono::Local::now().timestamp_millis();
            let elapsed = run_state_clone.start_time.elapsed().as_secs();
            let counters = run_state_clone.counters.read().unwrap_or_else(|e| e.into_inner()).clone();
            let top_compression = run_state_clone.top_compression.read().unwrap_or_else(|e| e.into_inner()).clone();
            let all_events: Vec<_> = run_state_clone.all_events.read().unwrap_or_else(|e| e.into_inner()).clone();
            let run_canceled = {
                let s = run_state_clone.status.read().unwrap_or_else(|e| e.into_inner());
                matches!(*s, ProcessingStatus::Cancelling)
            };

            let summary = RunSummary {
                counters,
                elapsed_secs: elapsed,
                top_compression,
                status_lines: vec![],
                source_folder: source_folder_str.clone(),
                start_ms,
                finish_ms,
                thread_count,
                max_retries,
                run_canceled,
            };
            let log_text = generate_efc_log(&summary, &all_events);
            let log_path = log_dir.join("flaccrunch.log");
            let _ = std::fs::write(&log_path, log_text.as_bytes());
        }

        let mut status = run_state_clone.status.write().unwrap_or_else(|e| e.into_inner());
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
