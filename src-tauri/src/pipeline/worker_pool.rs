use crate::pipeline::job::{execute_job, ProcessingContext};
use crate::pipeline::queue::JobQueue;
use crate::pipeline::stages::PipelineEvent;
use crate::state::run_state::{FileEvent, FileStatus, RunCounters, RunState, WorkerState};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use tauri::Emitter;

/// Run the worker pool: spawns N workers that pull from the queue and process files.
pub async fn run_worker_pool(
    worker_count: usize,
    queue: Arc<JobQueue>,
    context: Arc<ProcessingContext>,
    run_state: Arc<RunState>,
    app_handle: tauri::AppHandle,
) {
    let (event_tx, mut event_rx) = mpsc::channel::<PipelineEvent>(256);

    // Spawn worker tasks
    let mut worker_handles = Vec::new();
    for worker_id in 0..worker_count {
        let queue = Arc::clone(&queue);
        let context = Arc::clone(&context);
        let cancel_token = run_state.cancel_token.clone();
        let event_tx = event_tx.clone();

        let handle = tokio::spawn(async move {
            worker_loop(worker_id, queue, context, event_tx, cancel_token.clone()).await;
        });
        worker_handles.push(handle);
    }

    // Drop our copy of the sender so the receiver closes when all workers finish
    drop(event_tx);

    // Event dispatcher: forward events to RunState and Tauri frontend
    while let Some(event) = event_rx.recv().await {
        let emit_event = match &event {
            PipelineEvent::WorkerStarted {
                worker_id,
                file,
                stage,
            } => {
                let initial_state = match stage {
                    crate::pipeline::stages::PipelineStage::Hashing(_) => WorkerState::Hashing,
                    _ => WorkerState::Converting,
                };
                run_state.update_worker(*worker_id, initial_state, Some(file.clone()), 0, String::new());
                event
            }
            PipelineEvent::WorkerProgress {
                worker_id,
                percent,
                ratio,
            } => {
                let workers = run_state.workers.read().unwrap_or_else(|e| e.into_inner());
                if let Some(w) = workers.get(*worker_id) {
                    let file = w.file.clone();
                    drop(workers);
                    run_state.update_worker(*worker_id, WorkerState::Converting, file, *percent, ratio.clone());
                }
                event
            }
            PipelineEvent::WorkerStageChanged { worker_id, stage } => {
                let new_state = match stage {
                    crate::pipeline::stages::PipelineStage::Converting => WorkerState::Converting,
                    crate::pipeline::stages::PipelineStage::Hashing(_) => WorkerState::Hashing,
                    crate::pipeline::stages::PipelineStage::Artwork => WorkerState::Artwork,
                    crate::pipeline::stages::PipelineStage::Finalizing => WorkerState::Finalizing,
                    crate::pipeline::stages::PipelineStage::Complete => WorkerState::Idle,
                };
                let workers = run_state.workers.read().unwrap_or_else(|e| e.into_inner());
                let file = workers.get(*worker_id).and_then(|w| w.file.clone());
                drop(workers);
                run_state.update_worker(*worker_id, new_state, file, 0, String::new());
                event
            }
            PipelineEvent::FileCompleted { worker_id, event: file_event, .. } => {
                // Android: write the compressed cache file back to the original content URI
                // BEFORE recording/emitting so that write-back failures are reflected in the
                // event status shown to the user (not silently swallowed).
                //
                // Strategy (Solution C — hybrid):
                //   1. Try writing back via Kotlin bridge (contentResolver.openOutputStream),
                //      the canonical Android SAF write API.
                //   2. If that fails, copy to app-private fc-output/ directory so the
                //      processed file is never lost — status stays OK with a detail note.
                //   3. If both fail, mark FAIL with the combined error message.
                #[cfg(target_os = "android")]
                let file_event = {
                    let mut ev = file_event.clone();
                    if ev.status == crate::state::run_state::FileStatus::OK {
                        match android_write_back(&app_handle, &ev.file) {
                            Ok(()) => { /* in-place replacement succeeded */ }
                            Err(primary_err) => {
                                match android_save_to_fc_output(&app_handle, &ev.file) {
                                    Ok(out_path) => {
                                        // Keep status OK — file is accessible in fc-output.
                                        ev.detail = format!(
                                            "Saved to fc-output (write-back failed: {primary_err}): {out_path}"
                                        );
                                    }
                                    Err(fallback_err) => {
                                        ev.status = crate::state::run_state::FileStatus::FAIL;
                                        ev.detail = format!(
                                            "Write-back failed: {primary_err}; fc-output fallback also failed: {fallback_err}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    ev
                };

                // Record the event in run state (updates counters)
                run_state.record_event(*file_event.clone());
                // Read updated counters and re-emit with snapshot
                let counters = run_state.counters.read().unwrap_or_else(|e| e.into_inner()).clone();
                let enriched = PipelineEvent::FileCompleted {
                    worker_id: *worker_id,
                    event: file_event.clone(),
                    counters,
                };
                // Emit enriched event instead of original
                let _ = app_handle.emit("pipeline-event", &enriched);

                continue;
            }
            PipelineEvent::WorkerHashComputed { .. } => {
                // Pass through to frontend unchanged — no run_state update needed
                event
            }
            PipelineEvent::WorkerIdle { worker_id } => {
                run_state.update_worker(*worker_id, WorkerState::Idle, None, 0, String::new());
                event
            }
            PipelineEvent::RunComplete => event,
        };

        // Emit event to frontend
        let _ = app_handle.emit("pipeline-event", &emit_event);
    }

    // Wait for all workers to complete
    for handle in worker_handles {
        let _ = handle.await;
    }

    // Emit RunComplete to frontend
    let _ = app_handle.emit("pipeline-event", &PipelineEvent::RunComplete);
}

async fn worker_loop(
    worker_id: usize,
    queue: Arc<JobQueue>,
    context: Arc<ProcessingContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
) {
    loop {
        if cancel_token.is_cancelled() {
            break;
        }

        let item = match queue.dequeue() {
            Some(item) => item,
            None => break, // Queue empty, worker done
        };

        let result = execute_job(&item, worker_id, &context, &event_tx, cancel_token.clone()).await;

        // Handle retry
        let file_event = make_file_event(&result);
        let dummy_counters = RunCounters::default();
        if result.status == FileStatus::FAIL && item.attempt < context.max_retries {
            queue.requeue_for_retry(item, result.attempt + 1);
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event: Box::new(file_event),
                    counters: dummy_counters,
                })
                .await;
        } else {
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event: Box::new(file_event),
                    counters: dummy_counters,
                })
                .await;
        }

        // Mark worker idle
        let _ = event_tx
            .send(PipelineEvent::WorkerIdle { worker_id })
            .await;
    }
}

/// Android only: write the compressed cache file back to the original content URI.
///
/// Delegates to the Kotlin AndroidBridgePlugin.writeCacheFileToUri command, which
/// calls contentResolver.openOutputStream(uri, "wt") — the canonical Android API
/// for overwriting a file granted via ACTION_OPEN_DOCUMENT.  This replaces the
/// previous tauri_plugin_fs approach, which did not reliably support write mode
/// for content:// URIs.
///
/// Real-path originals (file:// or bare path, rare on modern Android) are still
/// written via std::fs::write.
#[cfg(target_os = "android")]
fn android_write_back<R: tauri::Runtime>(app: &tauri::AppHandle<R>, cache_path: &str) -> Result<(), String> {
    use tauri::Manager;

    let state = app.state::<crate::state::app_state::AppState>();
    let original_uri = {
        let map = state.content_uri_map.read().unwrap_or_else(|e| e.into_inner());
        map.get(cache_path).cloned()
    };
    // No mapping means the file was never cached (shouldn't happen, but guard anyway).
    let Some(uri_str) = original_uri else { return Ok(()) };

    // Real filesystem path (file:// or bare path): write directly.
    if !uri_str.starts_with("content://") {
        let data = std::fs::read(cache_path)
            .map_err(|e| format!("Failed to read cache file: {e}"))?;
        return std::fs::write(&uri_str, &data)
            .map_err(|e| format!("Failed to write back to path '{uri_str}': {e}"));
    }

    // Content URI: delegate to Kotlin via AndroidBridge.
    // contentResolver.openOutputStream(uri, "wt") handles all SAF providers correctly,
    // including MediaStore on Android 11+ where tauri_plugin_fs write mode fails.
    app.try_state::<crate::android_bridge::AndroidBridge>()
        .ok_or_else(|| "AndroidBridge not registered — cannot write to content URI".to_string())?
        .write_cache_to_uri(cache_path, &uri_str)
}

/// Android only: copy the compressed cache file to the app-private fc-output directory.
///
/// Used as a fallback when write-back to the original content URI fails.  The app
/// always has write access to its own data directory — no permissions required.
/// Files land at: <app_local_data_dir>/fc-output/<filename>
/// Visible in the Android Files app under: Android/data/com.flaccrunch.app/files/fc-output/
#[cfg(target_os = "android")]
fn android_save_to_fc_output<R: tauri::Runtime>(app: &tauri::AppHandle<R>, cache_path: &str) -> Result<String, String> {
    use tauri::Manager;

    let output_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to get app local data dir: {e}"))?
        .join("fc-output");

    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create fc-output dir: {e}"))?;

    // The cache file was already named with the real display name by resolve_filepath,
    // so we can use it directly as the output filename.
    let filename = std::path::Path::new(cache_path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Cannot derive filename from cache path: {cache_path}"))?;

    let dest = output_dir.join(filename);

    std::fs::copy(cache_path, &dest)
        .map_err(|e| format!("Failed to copy to fc-output: {e}"))?;

    Ok(dest.display().to_string())
}

fn make_file_event(
    result: &crate::pipeline::stages::JobResult,
) -> FileEvent {
    FileEvent {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        status: result.status.clone(),
        file: result.file_path.clone(),
        attempt: format!("{}", result.attempt),
        verification: result.verification.clone(),
        before_size: result.before_size,
        after_size: result.after_size,
        saved_bytes: result.saved_bytes,
        compression_pct: result.compression_pct,
        detail: result.error.clone().unwrap_or_default(),
        source_hash: result.source_hash.clone(),
        output_hash: result.output_hash.clone(),
        embedded_md5: result.embedded_md5.clone(),
        artwork_saved_bytes: result.artwork_result.as_ref().map(|a| a.saved_bytes).unwrap_or(0),
        artwork_raw_saved_bytes: result.artwork_result.as_ref().map(|a| a.raw_saved_bytes).unwrap_or(0),
        artwork_blocks_optimized: result.artwork_result.as_ref().map(|a| a.blocks_optimized).unwrap_or(0),
    }
}
