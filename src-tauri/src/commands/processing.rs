use crate::fs::scanner::{cleanup_stale_temps, scan_for_flac_files};
use crate::fs::tempfile::test_directory_write_access;
use crate::logging::efc_log::generate_efc_log;
use crate::pipeline::job::ProcessingContext;
use crate::pipeline::queue::JobQueue;
use crate::pipeline::sink::EventSink;
use crate::pipeline::stages::PipelineEvent;
use crate::pipeline::worker_pool::run_worker_pool;
use crate::state::app_state::AppState;
use crate::state::run_state::{FileEvent, ProcessingStatus, RunState, RunSummary};
use crate::state::settings::ProcessingSettings;
use crate::util::platform::default_log_folder;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, State};

/// Name of the Tauri event every pipeline event is delivered on.
pub const PIPELINE_EVENT: &str = "pipeline-event";

/// Forwards pipeline events to the webview. On Android it also writes the
/// compressed cache file back to the original content URI.
struct GuiSink {
    app: tauri::AppHandle,
}

impl EventSink for GuiSink {
    fn emit(&self, event: &PipelineEvent) {
        let _ = self.app.emit(PIPELINE_EVENT, event);
    }

    #[cfg(target_os = "android")]
    fn finalize_file_event(&self, event: &mut FileEvent) {
        use crate::state::run_state::FileStatus;
        if event.status != FileStatus::OK {
            return;
        }
        match android_write_back(&self.app, &event.file) {
            Ok(()) => {
                // Written back to the original document URI; drop the cached
                // input so the app's cache does not bloat.
                let _ = std::fs::remove_file(&event.file);
            }
            Err(err) => {
                event.status = FileStatus::FAIL;
                event.detail = format!("Write back to original file failed: {err}");
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    fn finalize_file_event(&self, _event: &mut FileEvent) {}

    fn run_finished(&self) {
        // Android: sweep the flac_input cache (leftovers from failed files)
        // and forget the URI map for this run.
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            if let Ok(cache_dir) = self.app.path().app_cache_dir() {
                let _ = std::fs::remove_dir_all(cache_dir.join("flac_input"));
            }
            let state = self.app.state::<AppState>();
            state
                .content_uri_map
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
    }
}

/// Android only: write the compressed cache file back to the original
/// document URI that the user selected via ACTION_OPEN_DOCUMENT.
#[cfg(target_os = "android")]
fn android_write_back(app: &tauri::AppHandle, cache_path: &str) -> Result<(), String> {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let bridge = app
        .try_state::<crate::android_bridge::AndroidBridge>()
        .ok_or_else(|| "AndroidBridge not available".to_string())?;

    let original_uri = state
        .content_uri_map
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(cache_path)
        .cloned()
        .ok_or_else(|| {
            format!("No original document URI recorded for cached file: {cache_path}")
        })?;

    bridge
        .write_file_to_uri(&original_uri, cache_path)
        .map_err(|e| format!("Failed to open original document for writing: {e}"))?;

    state
        .content_uri_map
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .remove(cache_path);
    Ok(())
}

/// Start processing FLAC files in the given folders.
#[tauri::command]
pub async fn start_processing(
    folders: Vec<String>,
    settings: ProcessingSettings,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    if let Some(run) = state.active_run() {
        if run.status_snapshot() == ProcessingStatus::Processing {
            return Err("Processing is already in progress".to_string());
        }
    }

    let folder_paths: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();

    // Fail fast with a clear error rather than discovering permission
    // problems mid-run (mirrors PowerShell's Test-DirectoryWriteAccess).
    let unwritable: Vec<String> = folder_paths
        .iter()
        .filter(|p| p.is_dir() && !test_directory_write_access(p))
        .map(|p| p.display().to_string())
        .collect();
    if !unwritable.is_empty() {
        return Err(format!(
            "Permission denied: cannot write to folder(s): {}. \
             Check that the folders are not read-only and that you have write access.",
            unwritable.join(", ")
        ));
    }

    let dir_paths: Vec<PathBuf> = folder_paths
        .iter()
        .filter(|p| p.is_dir())
        .cloned()
        .collect();
    cleanup_stale_temps(&dir_paths);

    let scan_result = scan_for_flac_files(&folder_paths);
    if scan_result.files.is_empty() {
        return Err("No FLAC files found in the selected folders".to_string());
    }

    let worker_count = settings.thread_count.clamp(1, scan_result.files.len());
    let run_id = format!(
        "run_{}_{}",
        chrono::Local::now().format("%Y%m%d_%H%M%S"),
        std::process::id()
    );

    // Resolve the verbose-log output folder (only used when verbose_logging is true)
    let verbose_log_dir: Option<PathBuf> = if settings.verbose_logging {
        let base = if settings.log_folder.is_empty() {
            #[cfg(mobile)]
            {
                use tauri::Manager;
                app.path()
                    .app_cache_dir()
                    .unwrap_or_else(|_| default_log_folder())
            }
            #[cfg(not(mobile))]
            {
                default_log_folder()
            }
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

    let run_state = Arc::new(RunState::new(run_id.clone(), worker_count));
    {
        let mut counters = run_state
            .counters
            .write()
            .unwrap_or_else(|e| e.into_inner());
        counters.total_files = scan_result.files.len();
        counters.total_original_bytes = scan_result.total_size;
    }
    run_state.set_status(ProcessingStatus::Processing);
    *state.active_run.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&run_state));

    let context = Arc::new(ProcessingContext {
        max_retries: settings.max_retries,
        mark_as_crunched: settings.mark_as_crunched,
        skip_crunched: settings.skip_crunched,
        profile: if settings.max_compression {
            crate::flac::encoder::CompressionProfile::max()
        } else {
            crate::flac::encoder::CompressionProfile::subset()
        },
        encoder_threads: crate::util::platform::encoder_threads_for(worker_count),
    });

    let queue = Arc::new(JobQueue::new(scan_result.files));
    *run_state.queue.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&queue));

    let source_folder_str = folders.join(", ");
    let thread_count = worker_count;
    let max_retries = settings.max_retries;
    let start_ms = chrono::Local::now().timestamp_millis();

    // Watchdog: closes the queue once it stays empty + idle for the grace
    // period, so workers can exit. Without this, workers would wait forever
    // for live drops that may never arrive.
    {
        let queue = Arc::clone(&queue);
        let run_state = Arc::clone(&run_state);
        tokio::spawn(async move {
            const GRACE: std::time::Duration = std::time::Duration::from_millis(1500);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                if run_state.cancel_token.is_cancelled() {
                    queue.close();
                    break;
                }
                if queue.is_empty() && run_state.all_workers_idle() && queue.idle_since() >= GRACE {
                    queue.close();
                    break;
                }
            }
        });
    }

    let sink: Arc<dyn EventSink> = Arc::new(GuiSink { app });
    tokio::spawn(async move {
        run_worker_pool(worker_count, queue, context, Arc::clone(&run_state), sink).await;

        if let Some(log_dir) = verbose_log_dir {
            let summary = RunSummary {
                counters: run_state.counters_snapshot(),
                elapsed_secs: run_state.start_time.elapsed().as_secs(),
                top_compression: run_state
                    .top_compression
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
                source_folder: source_folder_str,
                start_ms,
                finish_ms: chrono::Local::now().timestamp_millis(),
                thread_count,
                max_retries,
                run_canceled: run_state.status_snapshot() == ProcessingStatus::Cancelling,
            };
            let all_events: Vec<_> = run_state
                .all_events
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let log_text = generate_efc_log(&summary, &all_events);
            let _ = std::fs::write(log_dir.join("flaccrunch.log"), log_text.as_bytes());
        }

        *run_state.queue.write().unwrap_or_else(|e| e.into_inner()) = None;
        run_state.set_status(ProcessingStatus::Complete);
    });

    Ok(run_id)
}

/// Append more files to a currently-running job queue. Used by the frontend
/// when a user drops folders/files onto the window mid-run.
///
/// Returns the number of FLAC files actually added (may be 0 if no FLACs were
/// found in the dropped paths, or if there is no active run).
#[tauri::command]
pub async fn enqueue_files(
    paths: Vec<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    let Some(run) = state.active_run() else {
        return Ok(0);
    };
    if run.status_snapshot() != ProcessingStatus::Processing {
        return Ok(0);
    }
    let queue = run.queue.read().unwrap_or_else(|e| e.into_inner()).clone();
    let Some(queue) = queue else {
        return Ok(0);
    };
    if queue.is_closed() {
        return Ok(0);
    }

    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let scan = scan_for_flac_files(&path_bufs);
    if scan.files.is_empty() {
        return Ok(0);
    }

    let added_size = scan.total_size;
    let added = queue.push_files(scan.files);
    if added == 0 {
        return Ok(0);
    }

    let counters = {
        let mut c = run.counters.write().unwrap_or_else(|e| e.into_inner());
        c.total_files += added;
        c.total_original_bytes += added_size;
        c.clone()
    };
    let _ = app.emit(
        PIPELINE_EVENT,
        &PipelineEvent::QueueGrew { added, counters },
    );
    Ok(added)
}

/// Cancel an active processing run.
#[tauri::command]
pub async fn cancel_processing(state: State<'_, AppState>) -> Result<(), String> {
    match state.active_run() {
        Some(run) => {
            run.set_status(ProcessingStatus::Cancelling);
            run.cancel_token.cancel();
            Ok(())
        }
        None => Err("No active processing run".to_string()),
    }
}
