use crate::artwork::optimize::ArtworkResult;
use crate::state::run_state::{FileEvent, FileStatus, RunCounters};
use serde::{Deserialize, Serialize};

/// The processing stage a worker is currently in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStage {
    Converting,
    Hashing(HashPhase),
    Artwork,
    Finalizing,
    Complete,
}

/// Which hash is being computed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HashPhase {
    Source,
    Output,
}

/// Events emitted by the pipeline. The GUI forwards them to the webview as
/// `pipeline-event`; the CLI renders them to the console.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PipelineEvent {
    WorkerStarted {
        worker_id: usize,
        file: String,
        stage: PipelineStage,
    },
    WorkerProgress {
        worker_id: usize,
        percent: u8,
        ratio: String,
    },
    WorkerStageChanged {
        worker_id: usize,
        stage: PipelineStage,
    },
    /// A file finished (OK / WARN / FAIL / SKIPPED) or was scheduled for a
    /// retry (RETRY). `counters` is filled in by the dispatcher with a snapshot
    /// taken after the event was recorded.
    FileCompleted {
        worker_id: usize,
        event: Box<FileEvent>,
        counters: RunCounters,
    },
    /// Emitted immediately after a hash finishes computing, so the UI can
    /// display the value live before the file is fully processed.
    WorkerHashComputed {
        worker_id: usize,
        phase: HashPhase,
        hash: String,
        /// Only present for the Source phase: the embedded STREAMINFO MD5.
        embedded_md5: Option<String>,
    },
    WorkerIdle {
        worker_id: usize,
    },
    /// Emitted when files were appended to a running queue (live drag-and-drop).
    /// Carries a counters snapshot so the FE can refresh `totalFiles` /
    /// `totalOriginalBytes` without a separate poll.
    QueueGrew {
        added: usize,
        counters: RunCounters,
    },
    RunComplete,
}

/// Result of processing a single FLAC file.
#[derive(Debug)]
pub struct JobResult {
    pub status: FileStatus,
    pub file_path: String,
    pub attempt: u32,
    pub before_size: u64,
    pub after_size: u64,
    pub saved_bytes: i64,
    pub compression_pct: f64,
    pub source_hash: Option<String>,
    pub output_hash: Option<String>,
    pub embedded_md5: Option<String>,
    pub verification: String,
    pub artwork_result: Option<ArtworkResult>,
    pub error: Option<String>,
}

impl JobResult {
    /// Convert into the serializable event recorded in run state and shown in
    /// the UI / logs.
    pub fn to_file_event(&self) -> FileEvent {
        let art = self.artwork_result.as_ref();
        FileEvent {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            status: self.status.clone(),
            file: self.file_path.clone(),
            attempt: self.attempt.to_string(),
            verification: self.verification.clone(),
            before_size: self.before_size,
            after_size: self.after_size,
            saved_bytes: self.saved_bytes,
            compression_pct: self.compression_pct,
            detail: self.error.clone().unwrap_or_default(),
            source_hash: self.source_hash.clone(),
            output_hash: self.output_hash.clone(),
            embedded_md5: self.embedded_md5.clone(),
            artwork_saved_bytes: art.map(|a| a.saved_bytes).unwrap_or(0),
            artwork_raw_saved_bytes: art.map(|a| a.raw_saved_bytes).unwrap_or(0),
            artwork_blocks_optimized: art.map(|a| a.blocks_optimized).unwrap_or(0),
            padding_saved_bytes: art.map(|a| a.padding_saved_bytes).unwrap_or(0),
        }
    }
}

/// Verification result after comparing hashes.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationResult {
    pub passed: bool,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_event_serialization_uses_camel_case_tags() {
        let event = PipelineEvent::WorkerStarted {
            worker_id: 0,
            file: "test.flac".to_string(),
            stage: PipelineStage::Converting,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"workerStarted""#), "{json}");
        assert!(json.contains(r#""workerId":0"#), "{json}");
        assert!(json.contains(r#""stage":"converting""#), "{json}");

        let event = PipelineEvent::WorkerStageChanged {
            worker_id: 0,
            stage: PipelineStage::Hashing(HashPhase::Output),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""stage":{"hashing":"output"}"#), "{json}");

        let json = serde_json::to_string(&PipelineEvent::RunComplete).unwrap();
        assert_eq!(json, r#"{"type":"runComplete"}"#);
    }

    #[test]
    fn test_job_result_to_file_event_copies_artwork_fields() {
        let result = JobResult {
            status: FileStatus::OK,
            file_path: "/music/a.flac".to_string(),
            attempt: 2,
            before_size: 1000,
            after_size: 800,
            saved_bytes: 200,
            compression_pct: 20.0,
            source_hash: Some("aa".into()),
            output_hash: Some("aa".into()),
            embedded_md5: None,
            verification: "MATCH|NEW".to_string(),
            artwork_result: Some(ArtworkResult {
                changed: true,
                saved_bytes: 50,
                raw_saved_bytes: 50,
                padding_saved_bytes: 30,
                blocks_optimized: 1,
                summary: String::new(),
            }),
            error: None,
        };
        let ev = result.to_file_event();
        assert_eq!(ev.attempt, "2");
        assert_eq!(ev.artwork_saved_bytes, 50);
        assert_eq!(ev.padding_saved_bytes, 30);
        assert_eq!(ev.artwork_blocks_optimized, 1);
        assert_eq!(ev.detail, "");
        assert_eq!(ev.verification, "MATCH|NEW");
    }

    #[test]
    fn test_job_result_to_file_event_without_artwork() {
        let result = JobResult {
            status: FileStatus::FAIL,
            file_path: "/music/a.flac".to_string(),
            attempt: 1,
            before_size: 1000,
            after_size: 0,
            saved_bytes: 0,
            compression_pct: 0.0,
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            verification: "N/A".to_string(),
            artwork_result: None,
            error: Some("boom".into()),
        };
        let ev = result.to_file_event();
        assert_eq!(ev.status, FileStatus::FAIL);
        assert_eq!(ev.detail, "boom");
        assert_eq!(ev.artwork_saved_bytes, 0);
        assert_eq!(ev.padding_saved_bytes, 0);
    }
}
