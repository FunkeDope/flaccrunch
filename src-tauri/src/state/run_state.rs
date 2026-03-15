use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Unique identifier for a processing run.
pub type RunId = String;

/// Overall processing status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingStatus {
    Idle,
    Scanning,
    Processing,
    Cancelling,
    Complete,
}

/// Current state of an individual worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    pub id: usize,
    pub state: WorkerState,
    pub file: Option<String>,
    pub percent: u8,
    pub ratio: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkerState {
    Idle,
    Converting,
    Hashing,
    Artwork,
    Finalizing,
}

/// An event representing the result of processing a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEvent {
    pub time: String,
    pub status: FileStatus,
    pub file: String,
    pub attempt: String,
    pub verification: String,
    pub before_size: u64,
    pub after_size: u64,
    pub saved_bytes: i64,
    pub compression_pct: f64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileStatus {
    OK,
    RETRY,
    FAIL,
}

/// A compression result for tracking top compressions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionResult {
    pub path: String,
    pub saved_bytes: i64,
    pub saved_pct: f64,
}

/// Counters tracking the progress of a processing run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCounters {
    pub total_files: usize,
    pub processed: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_original_bytes: u64,
    pub total_new_bytes: u64,
    pub total_saved_bytes: i64,
    pub total_metadata_saved: i64,
    pub total_padding_saved: i64,
    pub total_artwork_saved: i64,
    pub total_artwork_raw_saved: i64,
    pub artwork_optimized_files: usize,
    pub artwork_optimized_blocks: usize,
}

/// Summary of a completed processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub counters: RunCounters,
    pub elapsed_secs: u64,
    pub top_compression: Vec<CompressionResult>,
    pub status_lines: Vec<String>,
}

/// Per-run state managed by the application.
pub struct RunState {
    pub id: RunId,
    pub status: RwLock<ProcessingStatus>,
    pub workers: RwLock<Vec<WorkerStatus>>,
    pub recent_events: RwLock<VecDeque<FileEvent>>,
    pub top_compression: RwLock<Vec<CompressionResult>>,
    pub counters: RwLock<RunCounters>,
    pub cancel_token: CancellationToken,
}

impl RunState {
    pub fn new(id: RunId, worker_count: usize) -> Self {
        let workers = (0..worker_count)
            .map(|i| WorkerStatus {
                id: i,
                state: WorkerState::Idle,
                file: None,
                percent: 0,
                ratio: String::new(),
            })
            .collect();

        Self {
            id,
            status: RwLock::new(ProcessingStatus::Scanning),
            workers: RwLock::new(workers),
            recent_events: RwLock::new(VecDeque::with_capacity(25)),
            top_compression: RwLock::new(Vec::new()),
            counters: RwLock::new(RunCounters::default()),
            cancel_token: CancellationToken::new(),
        }
    }

    /// Add a file event and update counters.
    pub fn record_event(&self, event: FileEvent) {
        // Update recent events (keep last 25)
        {
            let mut events = self.recent_events.write().unwrap();
            events.push_back(event.clone());
            while events.len() > 25 {
                events.pop_front();
            }
        }

        // Update counters
        {
            let mut counters = self.counters.write().unwrap();
            counters.processed += 1;
            match event.status {
                FileStatus::OK => {
                    counters.successful += 1;
                    counters.total_original_bytes += event.before_size;
                    counters.total_new_bytes += event.after_size;
                    counters.total_saved_bytes += event.saved_bytes;
                }
                FileStatus::FAIL => {
                    counters.failed += 1;
                }
                FileStatus::RETRY => {
                    // Retries don't count as processed yet
                    counters.processed -= 1;
                }
            }
        }

        // Update top compression
        if event.status == FileStatus::OK && event.saved_bytes > 0 {
            let mut top = self.top_compression.write().unwrap();
            top.push(CompressionResult {
                path: event.file,
                saved_bytes: event.saved_bytes,
                saved_pct: event.compression_pct,
            });
            top.sort_by(|a, b| b.saved_bytes.cmp(&a.saved_bytes));
            top.truncate(3);
        }
    }

    /// Update a specific worker's status.
    pub fn update_worker(&self, worker_id: usize, state: WorkerState, file: Option<String>, percent: u8, ratio: String) {
        let mut workers = self.workers.write().unwrap();
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.state = state;
            worker.file = file;
            worker.percent = percent;
            worker.ratio = ratio;
        }
    }
}
