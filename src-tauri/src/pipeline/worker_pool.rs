use crate::pipeline::job::{execute_job, ProcessingContext};
use crate::pipeline::queue::JobQueue;
use crate::pipeline::stages::PipelineEvent;
use crate::state::run_state::{FileEvent, FileStatus, RunState, WorkerState};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
        match &event {
            PipelineEvent::WorkerStarted {
                worker_id,
                file,
                stage: _,
            } => {
                run_state.update_worker(*worker_id, WorkerState::Converting, Some(file.clone()), 0, String::new());
            }
            PipelineEvent::WorkerProgress {
                worker_id,
                percent,
                ratio,
            } => {
                let workers = run_state.workers.read().unwrap();
                if let Some(w) = workers.get(*worker_id) {
                    let file = w.file.clone();
                    drop(workers);
                    run_state.update_worker(*worker_id, WorkerState::Converting, file, *percent, ratio.clone());
                }
            }
            PipelineEvent::WorkerStageChanged { worker_id, stage } => {
                let new_state = match stage {
                    crate::pipeline::stages::PipelineStage::Converting => WorkerState::Converting,
                    crate::pipeline::stages::PipelineStage::Hashing(_) => WorkerState::Hashing,
                    crate::pipeline::stages::PipelineStage::Artwork => WorkerState::Artwork,
                    crate::pipeline::stages::PipelineStage::Finalizing => WorkerState::Finalizing,
                    crate::pipeline::stages::PipelineStage::Complete => WorkerState::Idle,
                };
                let workers = run_state.workers.read().unwrap();
                let file = workers.get(*worker_id).and_then(|w| w.file.clone());
                drop(workers);
                run_state.update_worker(*worker_id, new_state, file, 0, String::new());
            }
            PipelineEvent::FileCompleted { worker_id: _, event } => {
                run_state.record_event(event.clone());
            }
            PipelineEvent::WorkerIdle { worker_id } => {
                run_state.update_worker(*worker_id, WorkerState::Idle, None, 0, String::new());
            }
            PipelineEvent::RunComplete => {}
        }

        // Emit event to frontend
        let _ = app_handle.emit("pipeline-event", &event);
    }

    // Wait for all workers to complete
    for handle in worker_handles {
        let _ = handle.await;
    }
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
        if result.status == FileStatus::FAIL && item.attempt < context.max_retries {
            queue.requeue_for_retry(item, result.attempt + 1);
            let event = make_file_event(&result, "RETRY");
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event,
                })
                .await;
        } else {
            let status_str = match result.status {
                FileStatus::OK => "OK",
                FileStatus::FAIL => "FAIL",
                FileStatus::RETRY => "RETRY",
            };
            let event = make_file_event(&result, status_str);
            let _ = event_tx
                .send(PipelineEvent::FileCompleted {
                    worker_id,
                    event,
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
    _status_str: &str,
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

use tauri::Emitter;
