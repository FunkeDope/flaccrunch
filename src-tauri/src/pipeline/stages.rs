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
#[serde(tag = "type", rename_all = "camelCase")]
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
        event: FileEvent,
        counters: RunCounters,
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
