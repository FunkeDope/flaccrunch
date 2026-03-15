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
            worker_loop(worker_id, queue, context, event_tx, cancel_token).await;
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
                stage: _,
            } => {
                run_state.update_worker(*worker_id, WorkerState::Converting, Some(file.clone()), 0, String::new());
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
                // Record the event in run state (updates counters)
                run_state.record_event(file_event.clone());
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

        let result = execute_job(&item, worker_id, &context, &event_tx).await;

        // Handle retry
        let file_event = make_file_event(&result);
        let dummy_counters = RunCounters::default();
        if result.status == FileStatus::FAIL && item.attempt < context.max_retries {
            queue.requeue_for_retry(item, result.attempt + 1);
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event: file_event,
                    counters: dummy_counters,
                })
                .await;
        } else {
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event: file_event,
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
    }
}
