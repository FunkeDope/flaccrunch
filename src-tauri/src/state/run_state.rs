use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
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
    /// Net bytes saved inside PICTURE blocks.
    pub artwork_saved_bytes: i64,
    pub artwork_raw_saved_bytes: i64,
    pub artwork_blocks_optimized: u32,
    /// Bytes of PADDING removed.
    #[serde(default)]
    pub padding_saved_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileStatus {
    OK,
    /// Compression succeeded but post-processing write-back to the final
    /// destination was incomplete; the original file was left unchanged.
    WARN,
    /// Attempt failed and has been re-queued. Not counted as processed.
    RETRY,
    FAIL,
    /// File already carries a FLACCRUNCH_INFO marker; processing was skipped.
    SKIPPED,
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
    /// Files that compressed OK but write-back to the intended destination
    /// was incomplete, leaving the original file unchanged.
    pub warned: usize,
    /// Files skipped because they already carried a FLACCRUNCH_INFO marker.
    pub skipped: usize,
}

/// Summary of a completed processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub counters: RunCounters,
    pub elapsed_secs: u64,
    pub top_compression: Vec<CompressionResult>,
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

/// Per-run state shared between the worker pool, the dispatcher and the UI.
pub struct RunState {
    pub id: RunId,
    pub status: RwLock<ProcessingStatus>,
    pub workers: RwLock<Vec<WorkerStatus>>,
    /// All events for the run — used for log generation.
    pub all_events: RwLock<Vec<FileEvent>>,
    pub top_compression: RwLock<Vec<CompressionResult>>,
    pub counters: RwLock<RunCounters>,
    pub cancel_token: CancellationToken,
    pub start_time: std::time::Instant,
    /// Reference to the active job queue. Set after the queue is built; used
    /// to append to a running run (live drag-and-drop).
    pub queue: RwLock<Option<Arc<crate::pipeline::queue::JobQueue>>>,
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
            all_events: RwLock::new(Vec::new()),
            top_compression: RwLock::new(Vec::new()),
            counters: RwLock::new(RunCounters::default()),
            cancel_token: CancellationToken::new(),
            start_time: std::time::Instant::now(),
            queue: RwLock::new(None),
        }
    }

    pub fn counters_snapshot(&self) -> RunCounters {
        self.counters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn status_snapshot(&self) -> ProcessingStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_status(&self, status: ProcessingStatus) {
        *self.status.write().unwrap_or_else(|e| e.into_inner()) = status;
    }

    pub fn all_workers_idle(&self) -> bool {
        self.workers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .all(|w| w.state == WorkerState::Idle)
    }

    /// Add a file event and update counters.
    pub fn record_event(&self, event: FileEvent) {
        self.all_events
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(event.clone());

        {
            let mut counters = self.counters.write().unwrap_or_else(|e| e.into_inner());
            match event.status {
                FileStatus::OK | FileStatus::WARN => {
                    counters.processed += 1;
                    counters.successful += 1;
                    if event.status == FileStatus::WARN {
                        counters.warned += 1;
                    }
                    // `total_original_bytes` is the size of everything queued
                    // (set from the scan, grown by live drops) and is NOT
                    // accumulated here — doing both halved the displayed
                    // savings percentage.
                    counters.total_new_bytes += event.after_size;
                    counters.total_saved_bytes += event.saved_bytes;
                    counters.total_artwork_saved += event.artwork_saved_bytes;
                    counters.total_artwork_raw_saved += event.artwork_raw_saved_bytes;
                    counters.total_padding_saved += event.padding_saved_bytes;
                    if event.artwork_blocks_optimized > 0 {
                        counters.artwork_optimized_files += 1;
                        counters.artwork_optimized_blocks +=
                            event.artwork_blocks_optimized as usize;
                    }
                }
                FileStatus::FAIL => {
                    counters.processed += 1;
                    counters.failed += 1;
                }
                FileStatus::SKIPPED => {
                    counters.processed += 1;
                    counters.skipped += 1;
                }
                // A retry is not an outcome; the file is still pending.
                FileStatus::RETRY => {}
            }
        }

        if (event.status == FileStatus::OK || event.status == FileStatus::WARN)
            && event.saved_bytes > 0
        {
            let mut top = self
                .top_compression
                .write()
                .unwrap_or_else(|e| e.into_inner());
            top.push(CompressionResult {
                path: event.file,
                saved_bytes: event.saved_bytes,
                saved_pct: event.compression_pct,
                before_size: event.before_size,
                after_size: event.after_size,
            });
            top.sort_by_key(|entry| std::cmp::Reverse(entry.saved_bytes));
            top.truncate(3);
        }
    }

    /// Replace a worker's full status.
    pub fn update_worker(
        &self,
        worker_id: usize,
        state: WorkerState,
        file: Option<String>,
        percent: u8,
        ratio: String,
    ) {
        let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.state = state;
            worker.file = file;
            worker.percent = percent;
            worker.ratio = ratio;
        }
    }

    /// Update only the stage of a worker (file is kept, progress reset).
    pub fn set_worker_state(&self, worker_id: usize, state: WorkerState) {
        let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.state = state;
            worker.percent = 0;
            worker.ratio.clear();
        }
    }

    /// Update only the encode progress of a worker.
    pub fn set_worker_progress(&self, worker_id: usize, percent: u8, ratio: String) {
        let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.state = WorkerState::Converting;
            worker.percent = percent;
            worker.ratio = ratio;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(status: FileStatus, file: &str, before: u64, after: u64) -> FileEvent {
        FileEvent {
            time: "12:00:00".to_string(),
            status,
            file: file.to_string(),
            attempt: "1".to_string(),
            verification: "OK".to_string(),
            before_size: before,
            after_size: after,
            saved_bytes: before as i64 - after as i64,
            compression_pct: if before > 0 {
                (before as f64 - after as f64) / before as f64 * 100.0
            } else {
                0.0
            },
            detail: String::new(),
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            artwork_saved_bytes: 0,
            artwork_raw_saved_bytes: 0,
            artwork_blocks_optimized: 0,
            padding_saved_bytes: 0,
        }
    }

    #[test]
    fn test_run_counters_default_all_zero() {
        let c = RunCounters::default();
        assert_eq!(c.total_files, 0);
        assert_eq!(c.processed, 0);
        assert_eq!(c.total_padding_saved, 0);
        assert_eq!(c.artwork_optimized_blocks, 0);
    }

    #[test]
    fn test_run_state_new_initialises_workers() {
        let rs = RunState::new("run-1".to_string(), 3);
        let workers = rs.workers.read().unwrap();
        assert_eq!(workers.len(), 3);
        for (i, w) in workers.iter().enumerate() {
            assert_eq!(w.id, i);
            assert_eq!(w.state, WorkerState::Idle);
            assert!(w.file.is_none());
        }
        assert_eq!(rs.status_snapshot(), ProcessingStatus::Scanning);
        assert!(rs.all_workers_idle());
    }

    #[test]
    fn test_record_ok_event_updates_counters() {
        let rs = RunState::new("run-4".to_string(), 1);
        // Set up front from the scan, as start_processing / the CLI do.
        rs.counters.write().unwrap().total_original_bytes = 1000;
        rs.record_event(make_event(FileStatus::OK, "/music/a.flac", 1000, 800));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 1);
        assert_eq!(c.failed, 0);
        assert_eq!(c.total_saved_bytes, 200);
        assert_eq!(c.total_original_bytes, 1000, "must not be double counted");
        assert_eq!(c.total_new_bytes, 800);
        assert_eq!(rs.all_events.read().unwrap().len(), 1);
    }

    #[test]
    fn test_record_fail_event_updates_counters() {
        let rs = RunState::new("run-5".to_string(), 1);
        rs.record_event(make_event(FileStatus::FAIL, "/music/bad.flac", 0, 0));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 0);
        assert_eq!(c.failed, 1);
    }

    #[test]
    fn test_record_retry_is_not_an_outcome() {
        let rs = RunState::new("run-6".to_string(), 1);
        rs.record_event(make_event(FileStatus::RETRY, "/music/retry.flac", 0, 0));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 0);
        assert_eq!(c.failed, 0);
        // …but it is still part of the event history for the log.
        assert_eq!(rs.all_events.read().unwrap().len(), 1);
    }

    #[test]
    fn test_retry_then_success_counts_once() {
        let rs = RunState::new("run-6b".to_string(), 1);
        rs.counters.write().unwrap().total_files = 1;
        rs.record_event(make_event(FileStatus::RETRY, "/a.flac", 0, 0));
        rs.record_event(make_event(FileStatus::RETRY, "/a.flac", 0, 0));
        rs.record_event(make_event(FileStatus::OK, "/a.flac", 100, 90));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 1);
        assert_eq!(c.failed, 0);
        assert_eq!(c.total_files, 1);
    }

    #[test]
    fn test_record_skipped_event_increments_skipped_only() {
        let rs = RunState::new("run-skip".to_string(), 1);
        rs.record_event(make_event(
            FileStatus::SKIPPED,
            "/music/already.flac",
            1000,
            1000,
        ));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 1);
        assert_eq!(c.skipped, 1);
        assert_eq!(c.successful, 0);
        assert_eq!(c.total_saved_bytes, 0);
    }

    #[test]
    fn test_record_warn_event_counts_as_successful_and_increments_warned() {
        let rs = RunState::new("run-warn".to_string(), 1);
        rs.record_event(make_event(FileStatus::WARN, "/music/a.flac", 1000, 900));
        let c = rs.counters_snapshot();
        assert_eq!(c.processed, 1);
        assert_eq!(c.successful, 1);
        assert_eq!(c.warned, 1);
        assert_eq!(c.total_saved_bytes, 100);
    }

    #[test]
    fn test_top_compression_sorted_and_truncated() {
        let rs = RunState::new("run-8".to_string(), 1);
        rs.record_event(make_event(FileStatus::OK, "/a.flac", 1000, 800));
        rs.record_event(make_event(FileStatus::OK, "/b.flac", 2000, 1400));
        rs.record_event(make_event(FileStatus::OK, "/c.flac", 3000, 2000));
        rs.record_event(make_event(FileStatus::OK, "/d.flac", 5000, 4500));
        let top = rs.top_compression.read().unwrap();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].saved_bytes, 1000);
        assert_eq!(top[1].saved_bytes, 600);
        assert_eq!(top[2].saved_bytes, 500);
    }

    #[test]
    fn test_ok_event_with_no_savings_does_not_enter_top_compression() {
        let rs = RunState::new("run-9".to_string(), 1);
        rs.record_event(make_event(FileStatus::OK, "/music/flat.flac", 1000, 1000));
        assert!(rs.top_compression.read().unwrap().is_empty());
    }

    #[test]
    fn test_artwork_and_padding_counters() {
        let rs = RunState::new("run-art".to_string(), 1);
        let mut event = make_event(FileStatus::OK, "/art.flac", 2000, 1800);
        event.artwork_saved_bytes = 50;
        event.artwork_raw_saved_bytes = 50;
        event.artwork_blocks_optimized = 2;
        event.padding_saved_bytes = 30;
        rs.record_event(event);
        let c = rs.counters_snapshot();
        assert_eq!(c.artwork_optimized_files, 1);
        assert_eq!(c.artwork_optimized_blocks, 2);
        assert_eq!(c.total_artwork_saved, 50);
        assert_eq!(c.total_padding_saved, 30);
    }

    #[test]
    fn test_worker_updates() {
        let rs = RunState::new("run-10".to_string(), 2);
        rs.update_worker(
            1,
            WorkerState::Hashing,
            Some("/x.flac".into()),
            0,
            String::new(),
        );
        rs.set_worker_progress(1, 50, "0.80".into());
        {
            let w = &rs.workers.read().unwrap()[1];
            assert_eq!(w.state, WorkerState::Converting);
            assert_eq!(w.file.as_deref(), Some("/x.flac"));
            assert_eq!(w.percent, 50);
            assert_eq!(w.ratio, "0.80");
        }
        rs.set_worker_state(1, WorkerState::Artwork);
        {
            let w = &rs.workers.read().unwrap()[1];
            assert_eq!(w.state, WorkerState::Artwork);
            assert_eq!(
                w.file.as_deref(),
                Some("/x.flac"),
                "file kept across stages"
            );
            assert_eq!(w.percent, 0);
        }
        assert!(!rs.all_workers_idle());
        // Out-of-range ids are ignored.
        rs.update_worker(99, WorkerState::Hashing, None, 0, String::new());
        assert_eq!(rs.workers.read().unwrap().len(), 2);
    }

    #[test]
    fn test_file_status_serde_roundtrip() {
        for status in [
            FileStatus::OK,
            FileStatus::WARN,
            FileStatus::FAIL,
            FileStatus::RETRY,
            FileStatus::SKIPPED,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: FileStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn test_file_event_deserializes_without_padding_field() {
        // Events round-tripped through an older frontend lack the new field.
        let json = r#"{"time":"t","status":"OK","file":"f","attempt":"1","verification":"MATCH",
            "beforeSize":10,"afterSize":9,"savedBytes":1,"compressionPct":10.0,"detail":"",
            "artworkSavedBytes":0,"artworkRawSavedBytes":0,"artworkBlocksOptimized":0}"#;
        let ev: FileEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev.padding_saved_bytes, 0);
    }

    #[test]
    fn test_run_counters_serde_uses_camel_case() {
        let json = serde_json::to_string(&RunCounters::default()).unwrap();
        assert!(json.contains("totalFiles"));
        assert!(json.contains("totalPaddingSaved"));
    }
}
