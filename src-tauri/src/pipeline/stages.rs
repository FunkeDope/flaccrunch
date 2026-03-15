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

/// Events emitted by the pipeline for UI updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
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

/// Verification result after comparing hashes.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub passed: bool,
    pub description: String,
    pub source_hash: Option<String>,
    pub output_hash: Option<String>,
    pub embedded_md5: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_event_serialization() {
        // WorkerStarted
        let event = PipelineEvent::WorkerStarted {
            worker_id: 0,
            file: "test.flac".to_string(),
            stage: PipelineStage::Converting,
        };
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("WorkerStarted: {}", json);
        assert!(json.contains(r#""type":"workerStarted""#), "type field wrong: {}", json);
        assert!(json.contains(r#""workerId":0"#), "worker_id field should be camelCase 'workerId': {}", json);

        // WorkerProgress
        let event = PipelineEvent::WorkerProgress {
            worker_id: 1,
            percent: 45,
            ratio: "45%".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("WorkerProgress: {}", json);

        // WorkerStageChanged with Hashing
        let event = PipelineEvent::WorkerStageChanged {
            worker_id: 0,
            stage: PipelineStage::Hashing(HashPhase::Source),
        };
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("WorkerStageChanged(Hashing): {}", json);

        // WorkerStageChanged with Artwork
        let event = PipelineEvent::WorkerStageChanged {
            worker_id: 0,
            stage: PipelineStage::Artwork,
        };
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("WorkerStageChanged(Artwork): {}", json);

        // WorkerIdle
        let event = PipelineEvent::WorkerIdle { worker_id: 2 };
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("WorkerIdle: {}", json);

        // RunComplete
        let event = PipelineEvent::RunComplete;
        let json = serde_json::to_string(&event).unwrap();
        eprintln!("RunComplete: {}", json);
    }
}
