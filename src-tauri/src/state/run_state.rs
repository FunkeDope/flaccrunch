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
    pub source_hash: Option<String>,
    pub output_hash: Option<String>,
    pub embedded_md5: Option<String>,
    pub artwork_saved_bytes: i64,
    pub artwork_raw_saved_bytes: i64,
    pub artwork_blocks_optimized: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileStatus {
    OK,
    /// Compression succeeded but write-back to the original location failed;
    /// the compressed file was saved to the app's fc-output directory instead.
    WARN,
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
    pub before_size: u64,
    pub after_size: u64,
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
    /// Files that compressed OK but write-back to the original URI failed;
    /// saved to app fc-output directory instead.
    pub warned: usize,
}

/// Summary of a completed processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub counters: RunCounters,
    pub elapsed_secs: u64,
    pub top_compression: Vec<CompressionResult>,
    pub status_lines: Vec<String>,
    /// Folder(s) that were processed (joined with ", " if multiple).
    pub source_folder: String,
    /// Unix timestamp (ms) when the run started.
    pub start_ms: i64,
    /// Unix timestamp (ms) when the run finished.
    pub finish_ms: i64,
    pub thread_count: usize,
    pub max_retries: u32,
    pub run_canceled: bool,
}

/// Per-run state managed by the application.
pub struct RunState {
    pub id: RunId,
    pub status: RwLock<ProcessingStatus>,
    pub workers: RwLock<Vec<WorkerStatus>>,
    /// Last 25 events — drives the UI recent-events table.
    pub recent_events: RwLock<VecDeque<FileEvent>>,
    /// All events for the run — used for log generation.
    pub all_events: RwLock<Vec<FileEvent>>,
    pub top_compression: RwLock<Vec<CompressionResult>>,
    pub counters: RwLock<RunCounters>,
    pub cancel_token: CancellationToken,
    pub start_time: std::time::Instant,
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
            all_events: RwLock::new(Vec::new()),
            top_compression: RwLock::new(Vec::new()),
            counters: RwLock::new(RunCounters::default()),
            cancel_token: CancellationToken::new(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Add a file event and update counters.
    pub fn record_event(&self, event: FileEvent) {
        // Keep full history for log generation
        {
            let mut all = self.all_events.write().unwrap_or_else(|e| e.into_inner());
            all.push(event.clone());
        }
        // Update recent events (keep last 25 for UI)
        {
            let mut events = self.recent_events.write().unwrap_or_else(|e| e.into_inner());
            events.push_back(event.clone());
            while events.len() > 25 {
                events.pop_front();
            }
        }

        // Update counters
        {
            let mut counters = self.counters.write().unwrap_or_else(|e| e.into_inner());
            counters.processed += 1;
            match event.status {
                FileStatus::OK | FileStatus::WARN => {
                    counters.successful += 1;
                    if event.status == FileStatus::WARN {
                        counters.warned += 1;
                    }
                    counters.total_original_bytes += event.before_size;
                    counters.total_new_bytes += event.after_size;
                    counters.total_saved_bytes += event.saved_bytes;
                    counters.total_artwork_saved += event.artwork_saved_bytes;
                    counters.total_artwork_raw_saved += event.artwork_raw_saved_bytes;
                    if event.artwork_blocks_optimized > 0 {
                        counters.artwork_optimized_files += 1;
                        counters.artwork_optimized_blocks += event.artwork_blocks_optimized as usize;
                    }
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
        if (event.status == FileStatus::OK || event.status == FileStatus::WARN) && event.saved_bytes > 0 {
            let mut top = self.top_compression.write().unwrap_or_else(|e| e.into_inner());
            top.push(CompressionResult {
                path: event.file,
                saved_bytes: event.saved_bytes,
                saved_pct: event.compression_pct,
                before_size: event.before_size,
                after_size: event.after_size,
            });
            top.sort_by(|a, b| b.saved_bytes.cmp(&a.saved_bytes));
            top.truncate(3);
        }
    }

    /// Update a specific worker's status.
    pub fn update_worker(&self, worker_id: usize, state: WorkerState, file: Option<String>, percent: u8, ratio: String) {
        let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.state = state;
            worker.file = file;
            worker.percent = percent;
            worker.ratio = ratio;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ok_event(file: &str, before: u64, after: u64, saved: i64, compression_pct: f64) -> FileEvent {
        FileEvent {
            time: "12:00:00".to_string(),
            status: FileStatus::OK,
            file: file.to_string(),
            attempt: "1".to_string(),
            verification: "OK".to_string(),
            before_size: before,
            after_size: after,
            saved_bytes: saved,
            compression_pct,
            detail: String::new(),
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            artwork_saved_bytes: 0,
            artwork_raw_saved_bytes: 0,
            artwork_blocks_optimized: 0,
        }
    }

    fn make_fail_event(file: &str) -> FileEvent {
        FileEvent {
            time: "12:00:00".to_string(),
            status: FileStatus::FAIL,
            file: file.to_string(),
            attempt: "3".to_string(),
            verification: String::new(),
            before_size: 0,
            after_size: 0,
            saved_bytes: 0,
            compression_pct: 0.0,
            detail: "some error".to_string(),
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            artwork_saved_bytes: 0,
            artwork_raw_saved_bytes: 0,
            artwork_blocks_optimized: 0,
        }
    }

    fn make_retry_event(file: &str) -> FileEvent {
        FileEvent {
            time: "12:00:00".to_string(),
            status: FileStatus::RETRY,
            file: file.to_string(),
            attempt: "1".to_string(),
            verification: String::new(),
            before_size: 0,
            after_size: 0,
            saved_bytes: 0,
            compression_pct: 0.0,
            detail: String::new(),
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            artwork_saved_bytes: 0,
            artwork_raw_saved_bytes: 0,
            artwork_blocks_optimized: 0,
        }
    }

    // --- RunCounters ---

    #[test]
    fn test_run_counters_default_all_zero() {
        let c = RunCounters::default();
        assert_eq!(c.total_files, 0);
        assert_eq!(c.processed, 0);
        assert_eq!(c.successful, 0);
        assert_eq!(c.failed, 0);
        assert_eq!(c.total_original_bytes, 0);
        assert_eq!(c.total_new_bytes, 0);
        assert_eq!(c.total_saved_bytes, 0);
        assert_eq!(c.total_metadata_saved, 0);
        assert_eq!(c.total_padding_saved, 0);
        assert_eq!(c.total_artwork_saved, 0);
        assert_eq!(c.total_artwork_raw_saved, 0);
        assert_eq!(c.artwork_optimized_files, 0);
        assert_eq!(c.artwork_optimized_blocks, 0);
    }

    #[test]
    fn test_run_counters_clone() {
        let mut c = RunCounters::default();
        c.total_files = 10;
        c.successful = 8;
        let c2 = c.clone();
        assert_eq!(c2.total_files, 10);
        assert_eq!(c2.successful, 8);
    }

    // --- FileStatus ---

    #[test]
    fn test_file_status_eq() {
        assert_eq!(FileStatus::OK, FileStatus::OK);
        assert_eq!(FileStatus::FAIL, FileStatus::FAIL);
        assert_eq!(FileStatus::RETRY, FileStatus::RETRY);
        assert_ne!(FileStatus::OK, FileStatus::FAIL);
    }

    #[test]
    fn test_file_status_clone() {
        let s = FileStatus::OK;
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    // --- WorkerState ---

    #[test]
    fn test_worker_state_variants_eq() {
        assert_eq!(WorkerState::Idle, WorkerState::Idle);
        assert_eq!(WorkerState::Converting, WorkerState::Converting);
        assert_eq!(WorkerState::Hashing, WorkerState::Hashing);
        assert_eq!(WorkerState::Artwork, WorkerState::Artwork);
        assert_eq!(WorkerState::Finalizing, WorkerState::Finalizing);
        assert_ne!(WorkerState::Idle, WorkerState::Converting);
    }

    #[test]
    fn test_worker_state_clone() {
        let ws = WorkerState::Hashing;
        assert_eq!(ws.clone(), WorkerState::Hashing);
    }

    // --- RunState construction ---

    #[test]
    fn test_run_state_new_initialises_workers() {
        let rs = RunState::new("run-1".to_string(), 3);
        let workers = rs.workers.read().unwrap();
        assert_eq!(workers.len(), 3);
        for (i, w) in workers.iter().enumerate() {
            assert_eq!(w.id, i);
            assert_eq!(w.state, WorkerState::Idle);
            assert!(w.file.is_none());
            assert_eq!(w.percent, 0);
            assert!(w.ratio.is_empty());
        }
    }

    #[test]
    fn test_run_state_initial_status_is_scanning() {
        let rs = RunState::new("run-2".to_string(), 1);
        let status = rs.status.read().unwrap();
        assert_eq!(*status, ProcessingStatus::Scanning);
    }

    #[test]
    fn test_run_state_counters_start_zero() {
        let rs = RunState::new("run-3".to_string(), 2);
        let counters = rs.counters.read().unwrap();
        assert_eq!(counters.processed, 0);
        assert_eq!(counters.successful, 0);
        assert_eq!(counters.failed, 0);
    }

    // --- RunState::record_event ---

    #[test]
    fn test_record_ok_event_updates_counters() {
        let rs = RunState::new("run-4".to_string(), 1);
        let event = make_ok_event("/music/a.flac", 1000, 800, 200, 20.0);
        rs.record_event(event);
        let c = rs.counters.read().unwrap();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 1);
        assert_eq!(c.failed, 0);
        assert_eq!(c.total_saved_bytes, 200);
        assert_eq!(c.total_original_bytes, 1000);
        assert_eq!(c.total_new_bytes, 800);
    }

    #[test]
    fn test_record_fail_event_updates_counters() {
        let rs = RunState::new("run-5".to_string(), 1);
        let event = make_fail_event("/music/bad.flac");
        rs.record_event(event);
        let c = rs.counters.read().unwrap();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 0);
        assert_eq!(c.failed, 1);
    }

    #[test]
    fn test_record_retry_does_not_increment_processed() {
        let rs = RunState::new("run-6".to_string(), 1);
        let event = make_retry_event("/music/retry.flac");
        rs.record_event(event);
        let c = rs.counters.read().unwrap();
        // RETRY increments then decrements processed, net 0
        assert_eq!(c.processed, 0);
    }

    #[test]
    fn test_recent_events_capped_at_25() {
        let rs = RunState::new("run-7".to_string(), 1);
        for i in 0..30 {
            let event = make_ok_event(&format!("/music/{i}.flac"), 1000, 900, 100, 10.0);
            rs.record_event(event);
        }
        let events = rs.recent_events.read().unwrap();
        assert_eq!(events.len(), 25);
    }

    #[test]
    fn test_top_compression_sorted_and_truncated() {
        let rs = RunState::new("run-8".to_string(), 1);
        rs.record_event(make_ok_event("/a.flac", 1000, 800, 200, 20.0));
        rs.record_event(make_ok_event("/b.flac", 2000, 1400, 600, 30.0));
        rs.record_event(make_ok_event("/c.flac", 3000, 2000, 1000, 33.3));
        rs.record_event(make_ok_event("/d.flac", 5000, 4500, 500, 10.0));
        let top = rs.top_compression.read().unwrap();
        assert_eq!(top.len(), 3);
        // should be sorted by saved_bytes descending: 1000, 600, 500
        assert_eq!(top[0].saved_bytes, 1000);
        assert_eq!(top[1].saved_bytes, 600);
        assert_eq!(top[2].saved_bytes, 500);
    }

    #[test]
    fn test_ok_event_with_no_savings_does_not_enter_top_compression() {
        let rs = RunState::new("run-9".to_string(), 1);
        let event = make_ok_event("/music/flat.flac", 1000, 1000, 0, 0.0);
        rs.record_event(event);
        let top = rs.top_compression.read().unwrap();
        assert!(top.is_empty());
    }

    // --- RunState::update_worker ---

    #[test]
    fn test_update_worker_changes_state() {
        let rs = RunState::new("run-10".to_string(), 2);
        rs.update_worker(1, WorkerState::Converting, Some("/music/x.flac".to_string()), 50, "0.80".to_string());
        let workers = rs.workers.read().unwrap();
        let w = &workers[1];
        assert_eq!(w.state, WorkerState::Converting);
        assert_eq!(w.file, Some("/music/x.flac".to_string()));
        assert_eq!(w.percent, 50);
        assert_eq!(w.ratio, "0.80");
    }

    #[test]
    fn test_update_worker_out_of_range_is_no_op() {
        let rs = RunState::new("run-11".to_string(), 2);
        // Worker id 99 does not exist; should not panic
        rs.update_worker(99, WorkerState::Hashing, None, 0, String::new());
        let workers = rs.workers.read().unwrap();
        assert_eq!(workers.len(), 2);
    }

    // --- ProcessingStatus ---

    #[test]
    fn test_processing_status_eq() {
        assert_eq!(ProcessingStatus::Idle, ProcessingStatus::Idle);
        assert_ne!(ProcessingStatus::Idle, ProcessingStatus::Processing);
    }

    // --- Serde roundtrips ---

    #[test]
    fn test_file_status_serde_roundtrip() {
        let statuses = [FileStatus::OK, FileStatus::WARN, FileStatus::FAIL, FileStatus::RETRY];
        for status in &statuses {
            let json = serde_json::to_string(status).expect("serialize");
            let back: FileStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(status, &back);
        }
    }

    #[test]
    fn test_record_warn_event_counts_as_successful_and_increments_warned() {
        let rs = RunState::new("run-warn".to_string(), 1);
        let mut event = make_ok_event("/music/a.flac", 1000, 900, 100, 10.0);
        event.status = FileStatus::WARN;
        event.detail = "Saved to fc-output (write-back failed: permission): /data/fc-output/a.flac".to_string();
        rs.record_event(event);
        let c = rs.counters.read().unwrap();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 1);
        assert_eq!(c.failed, 0);
        assert_eq!(c.warned, 1);
        assert_eq!(c.total_saved_bytes, 100);
    }

    #[test]
    fn test_worker_state_serde_roundtrip() {
        let states = [
            WorkerState::Idle,
            WorkerState::Converting,
            WorkerState::Hashing,
            WorkerState::Artwork,
            WorkerState::Finalizing,
        ];
        for state in &states {
            let json = serde_json::to_string(state).expect("serialize");
            let back: WorkerState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(state, &back);
        }
    }

    #[test]
    fn test_run_counters_serde_roundtrip() {
        let mut c = RunCounters::default();
        c.total_files = 5;
        c.successful = 4;
        c.failed = 1;
        let json = serde_json::to_string(&c).expect("serialize");
        let back: RunCounters = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.total_files, 5);
        assert_eq!(back.successful, 4);
        assert_eq!(back.failed, 1);
    }

    #[test]
    fn test_artwork_blocks_optimized_counter() {
        let rs = RunState::new("run-art".to_string(), 1);
        let mut event = make_ok_event("/art.flac", 2000, 1800, 200, 10.0);
        event.artwork_saved_bytes = 50;
        event.artwork_raw_saved_bytes = 60;
        event.artwork_blocks_optimized = 2;
        rs.record_event(event);
        let c = rs.counters.read().unwrap();
        assert_eq!(c.artwork_optimized_files, 1);
        assert_eq!(c.artwork_optimized_blocks, 2);
        assert_eq!(c.total_artwork_saved, 50);
        assert_eq!(c.total_artwork_raw_saved, 60);
    }
}
