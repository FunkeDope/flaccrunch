//! Worker pool: N workers pull from the shared queue, run the per-file
//! pipeline, and report back over a channel. A single dispatcher task applies
//! every event to `RunState` and forwards it to the `EventSink`.
//!
//! Shared by the GUI and the CLI.

use crate::pipeline::job::{execute_job, ProcessingContext};
use crate::pipeline::queue::JobQueue;
use crate::pipeline::sink::EventSink;
use crate::pipeline::stages::{PipelineEvent, PipelineStage};
use crate::state::run_state::{FileStatus, RunCounters, RunState, WorkerState};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How long an idle worker sleeps before re-checking an empty-but-open queue.
const QUEUE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// Run the worker pool to completion. Returns when every worker has exited
/// (queue closed and drained, or cancelled) and `RunComplete` has been emitted.
pub async fn run_worker_pool(
    worker_count: usize,
    queue: Arc<JobQueue>,
    context: Arc<ProcessingContext>,
    run_state: Arc<RunState>,
    sink: Arc<dyn EventSink>,
) {
    let (event_tx, mut event_rx) = mpsc::channel::<PipelineEvent>(256);

    let mut worker_handles = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let queue = Arc::clone(&queue);
        let context = Arc::clone(&context);
        let cancel_token = run_state.cancel_token.clone();
        let event_tx = event_tx.clone();
        worker_handles.push(tokio::spawn(async move {
            worker_loop(worker_id, queue, context, event_tx, cancel_token).await;
        }));
    }
    // Drop our copy so the receiver closes once every worker is done.
    drop(event_tx);

    while let Some(event) = event_rx.recv().await {
        dispatch(event, &run_state, sink.as_ref());
    }

    for handle in worker_handles {
        let _ = handle.await;
    }

    sink.run_finished();
    sink.emit(&PipelineEvent::RunComplete);
}

/// Apply one event to run state, then forward it.
fn dispatch(event: PipelineEvent, run_state: &RunState, sink: &dyn EventSink) {
    match event {
        PipelineEvent::WorkerStarted {
            worker_id,
            ref file,
            ref stage,
        } => {
            run_state.update_worker(
                worker_id,
                stage_to_state(stage),
                Some(file.clone()),
                0,
                String::new(),
            );
            sink.emit(&event);
        }
        PipelineEvent::WorkerProgress {
            worker_id,
            percent,
            ref ratio,
        } => {
            run_state.set_worker_progress(worker_id, percent, ratio.clone());
            sink.emit(&event);
        }
        PipelineEvent::WorkerStageChanged {
            worker_id,
            ref stage,
        } => {
            run_state.set_worker_state(worker_id, stage_to_state(stage));
            sink.emit(&event);
        }
        PipelineEvent::FileCompleted {
            worker_id,
            event: mut file_event,
            ..
        } => {
            sink.finalize_file_event(&mut file_event);
            run_state.record_event((*file_event).clone());
            let counters = run_state.counters_snapshot();
            sink.emit(&PipelineEvent::FileCompleted {
                worker_id,
                event: file_event,
                counters,
            });
        }
        PipelineEvent::WorkerIdle { worker_id } => {
            run_state.update_worker(worker_id, WorkerState::Idle, None, 0, String::new());
            sink.emit(&event);
        }
        // Pass-through events: no run-state bookkeeping needed.
        PipelineEvent::WorkerHashComputed { .. }
        | PipelineEvent::QueueGrew { .. }
        | PipelineEvent::RunComplete => sink.emit(&event),
    }
}

fn stage_to_state(stage: &PipelineStage) -> WorkerState {
    match stage {
        PipelineStage::Converting => WorkerState::Converting,
        PipelineStage::Hashing(_) => WorkerState::Hashing,
        PipelineStage::Artwork => WorkerState::Artwork,
        PipelineStage::Finalizing => WorkerState::Finalizing,
        PipelineStage::Complete => WorkerState::Idle,
    }
}

/// One worker: pull items until the queue is closed and drained (or the run
/// is cancelled), run each through the pipeline, schedule retries.
pub async fn worker_loop(
    worker_id: usize,
    queue: Arc<JobQueue>,
    context: Arc<ProcessingContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
) {
    loop {
        // Wait for an item, allowing the queue to grow mid-run (live drops).
        let item = loop {
            if cancel_token.is_cancelled() {
                return;
            }
            if let Some(item) = queue.dequeue() {
                break item;
            }
            if queue.is_closed() {
                return;
            }
            tokio::time::sleep(QUEUE_POLL_INTERVAL).await;
        };

        let result = execute_job(&item, worker_id, &context, &event_tx, cancel_token.clone()).await;

        // A failure is retried unless the run was cancelled or the attempt
        // budget is exhausted. SKIPPED is terminal and never retries.
        let mut file_event = result.to_file_event();
        let will_retry = result.status == FileStatus::FAIL
            && !cancel_token.is_cancelled()
            && item.attempt < context.max_retries;
        if will_retry {
            file_event.status = FileStatus::RETRY;
            queue.requeue_for_retry(item, result.attempt + 1);
        }

        let _ = event_tx
            .send(PipelineEvent::FileCompleted {
                worker_id,
                event: Box::new(file_event),
                counters: RunCounters::default(),
            })
            .await;

        queue.touch();
        let _ = event_tx.send(PipelineEvent::WorkerIdle { worker_id }).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::scanner::ScannedFile;
    use crate::state::run_state::FileEvent;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Collects every emitted event for assertions.
    struct RecordingSink {
        events: Mutex<Vec<PipelineEvent>>,
        finished: std::sync::atomic::AtomicBool,
    }

    impl EventSink for RecordingSink {
        fn emit(&self, event: &PipelineEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
        fn finalize_file_event(&self, event: &mut FileEvent) {
            // Simulate a platform hook that downgrades OK → WARN.
            if event.status == FileStatus::OK {
                event.status = FileStatus::WARN;
                event.detail = "hooked".into();
            }
        }
        fn run_finished(&self) {
            self.finished
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn missing_file(name: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/definitely/not/here/{name}")),
            name: name.to_string(),
            size: 10,
        }
    }

    /// Nonexistent inputs fail fast in the pipeline, which exercises the
    /// retry → RETRY → FAIL bookkeeping without needing libFLAC fixtures.
    #[tokio::test]
    async fn test_pool_retries_failures_then_records_final_fail() {
        let queue = Arc::new(JobQueue::new(vec![missing_file("a.flac")]));
        queue.close();
        let run_state = Arc::new(RunState::new("t".into(), 1));
        run_state.counters.write().unwrap().total_files = 1;
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
            finished: Default::default(),
        });
        let ctx = Arc::new(ProcessingContext {
            max_retries: 3,
            mark_as_crunched: false,
            skip_crunched: false,
            profile: Default::default(),
            encoder_threads: 1,
        });

        run_worker_pool(1, queue, ctx, Arc::clone(&run_state), sink.clone()).await;

        let events = sink.events.lock().unwrap();
        let statuses: Vec<FileStatus> = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::FileCompleted { event, .. } => Some(event.status.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            vec![FileStatus::RETRY, FileStatus::RETRY, FileStatus::FAIL],
            "attempts 1 and 2 retry, attempt 3 is terminal"
        );
        assert!(matches!(events.last(), Some(PipelineEvent::RunComplete)));
        assert!(sink.finished.load(std::sync::atomic::Ordering::SeqCst));

        let c = run_state.counters_snapshot();
        assert_eq!(c.processed, 1, "retries must not count as processed");
        assert_eq!(c.failed, 1);
        assert_eq!(c.successful, 0);

        // The last FileCompleted carries the dispatcher's counters snapshot.
        let last_completed = events
            .iter()
            .rev()
            .find_map(|e| match e {
                PipelineEvent::FileCompleted { counters, .. } => Some(counters.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(last_completed.failed, 1);
    }

    #[tokio::test]
    async fn test_pool_does_not_retry_when_cancelled() {
        let queue = Arc::new(JobQueue::new(vec![missing_file("a.flac")]));
        queue.close();
        let run_state = Arc::new(RunState::new("t".into(), 1));
        // Cancel before starting: the job fails immediately and must not be
        // rescheduled.
        run_state.cancel_token.cancel();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
            finished: Default::default(),
        });
        let ctx = Arc::new(ProcessingContext {
            max_retries: 3,
            mark_as_crunched: false,
            skip_crunched: false,
            profile: Default::default(),
            encoder_threads: 1,
        });

        run_worker_pool(1, queue.clone(), ctx, run_state, sink.clone()).await;

        let events = sink.events.lock().unwrap();
        let completed = events
            .iter()
            .filter(|e| matches!(e, PipelineEvent::FileCompleted { .. }))
            .count();
        assert_eq!(completed, 0, "cancelled worker exits before taking work");
        assert_eq!(queue.remaining(), 1);
    }
}
