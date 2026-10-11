//! The per-file pipeline: hash → encode → artwork → hash → replace.

// `Result<_, JobResult>` is used for early exit; the Err value is produced
// once per file, so its size is irrelevant.
#![allow(clippy::result_large_err)]

use crate::artwork::optimize::optimize_album_art;
use crate::flac::encoder::{
    encode_flac, CompressionProfile, EncodeError, EncodeOptions, EncodeProgress,
};
use crate::flac::hasher::hash_decoded_audio;
use crate::flac::marker;
use crate::flac::metadata::{extract_non_flac_prefix, get_md5sum, prepend_bytes_to_file};
use crate::fs::metadata::{restore_metadata, snapshot_metadata};
use crate::fs::tempfile::{safe_move, safe_remove, temp_path_for};
use crate::pipeline::queue::QueueItem;
use crate::pipeline::stages::{
    HashPhase, JobResult, PipelineEvent, PipelineStage, VerificationResult,
};
use crate::state::run_state::FileStatus;
use crate::util::format::NULL_MD5;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Context shared across all jobs in a processing run.
pub struct ProcessingContext {
    pub max_retries: u32,
    /// Stamp re-encoded files with a FLACCRUNCH_INFO Vorbis comment.
    pub mark_as_crunched: bool,
    /// Skip files that already carry a FLACCRUNCH_INFO marker.
    pub skip_crunched: bool,
    /// Encoder tuning (subset by default).
    pub profile: CompressionProfile,
    /// libFLAC threads per file (1 = plain; >1 fills idle cores when the
    /// queue is shorter than the worker count).
    pub encoder_threads: u32,
}

/// How often encoder progress is sampled and forwarded as an event.
const PROGRESS_POLL_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_millis(100);

/// Execute the full processing pipeline for a single FLAC file.
///
/// `cancel_token` is checked between stages and mirrored into an atomic flag
/// that the encoder thread polls, so cancellation aborts promptly.
pub async fn execute_job(
    item: &QueueItem,
    worker_id: usize,
    context: &ProcessingContext,
    event_tx: &mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
) -> JobResult {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_watcher = {
        let flag = cancel_flag.clone();
        let token = cancel_token.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            flag.store(true, Ordering::Relaxed);
        })
    };

    let job = Job {
        item,
        temp_path: temp_path_for(&item.file.path),
        worker_id,
        context,
        event_tx,
        cancel_token,
        cancel_flag,
    };

    let result = job.run().await.unwrap_or_else(|failure| failure);
    cancel_watcher.abort();

    // Whatever happened, a failed job must not leave its temp file behind.
    if result.status == FileStatus::FAIL {
        safe_remove(&job.temp_path);
    }
    result
}

struct Job<'a> {
    item: &'a QueueItem,
    temp_path: PathBuf,
    worker_id: usize,
    context: &'a ProcessingContext,
    event_tx: &'a mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
    cancel_flag: Arc<AtomicBool>,
}

impl Job<'_> {
    fn file_path(&self) -> &Path {
        &self.item.file.path
    }

    async fn emit(&self, event: PipelineEvent) {
        let _ = self.event_tx.send(event).await;
    }

    async fn stage(&self, stage: PipelineStage) {
        self.emit(PipelineEvent::WorkerStageChanged {
            worker_id: self.worker_id,
            stage,
        })
        .await;
    }

    fn fail(&self, error: impl Into<String>) -> JobResult {
        JobResult {
            status: FileStatus::FAIL,
            file_path: self.file_path().to_string_lossy().to_string(),
            attempt: self.item.attempt,
            before_size: self.item.file.size,
            after_size: 0,
            saved_bytes: 0,
            compression_pct: 0.0,
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            verification: "N/A".to_string(),
            artwork_result: None,
            error: Some(error.into()),
        }
    }

    fn skipped(&self) -> JobResult {
        JobResult {
            status: FileStatus::SKIPPED,
            file_path: self.file_path().to_string_lossy().to_string(),
            attempt: self.item.attempt,
            before_size: self.item.file.size,
            after_size: self.item.file.size,
            saved_bytes: 0,
            compression_pct: 0.0,
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            verification: "SKIPPED".to_string(),
            artwork_result: None,
            error: Some("Already crunched".to_string()),
        }
    }

    /// `Err` short-circuits with a failure result (used with `?`).
    fn check_cancel(&self) -> Result<(), JobResult> {
        if self.cancel_token.is_cancelled() {
            Err(self.fail("Cancelled"))
        } else {
            Ok(())
        }
    }

    async fn run(&self) -> Result<JobResult, JobResult> {
        let file_path = self.file_path();
        let file_name = &self.item.file.name;
        let before_size = self.item.file.size;
        let metadata_snapshot = snapshot_metadata(file_path).ok();

        // ── STAGE 1: PRE-HASH ────────────────────────────────────────────
        // Read the embedded MD5 and hash the original decoded audio before
        // any modification. This is the ground truth we verify against.
        self.emit(PipelineEvent::WorkerStarted {
            worker_id: self.worker_id,
            file: file_name.to_string(),
            stage: PipelineStage::Hashing(HashPhase::Source),
        })
        .await;

        if self.context.skip_crunched && marker::read_crunched_marker(file_path).unwrap_or(false) {
            return Ok(self.skipped());
        }

        let embedded_md5 = get_md5sum(file_path).await.ok();
        let source_hash = hash_decoded_audio(file_path).await.ok();
        if let Some(h) = &source_hash {
            self.emit(PipelineEvent::WorkerHashComputed {
                worker_id: self.worker_id,
                phase: HashPhase::Source,
                hash: h.clone(),
                embedded_md5: embedded_md5.clone(),
            })
            .await;
        }
        self.check_cancel()?;

        // ── STAGE 2: CONVERTING ──────────────────────────────────────────
        // Streaming re-encode with all metadata attached in the same pass.
        self.stage(PipelineStage::Converting).await;

        // Bytes before "fLaC" (e.g. an ID3v2 tag) are not part of the FLAC
        // stream; libFLAC drops them, so preserve and re-attach verbatim.
        let non_flac_prefix = extract_non_flac_prefix(file_path).unwrap_or_default();

        let progress = Arc::new(EncodeProgress::new());
        let poll_task = {
            let tx = self.event_tx.clone();
            let progress = progress.clone();
            let worker_id = self.worker_id;
            tokio::spawn(async move {
                let mut last = 0u8;
                loop {
                    tokio::time::sleep(PROGRESS_POLL_INTERVAL).await;
                    let current = progress.percent();
                    if current != last {
                        last = current;
                        let _ = tx
                            .send(PipelineEvent::WorkerProgress {
                                worker_id,
                                percent: current,
                                ratio: format!("{:.3}", progress.ratio()),
                            })
                            .await;
                    }
                }
            })
        };

        let encode_result = encode_flac(
            file_path,
            &self.temp_path,
            EncodeOptions {
                mark_as_crunched: self.context.mark_as_crunched,
                profile: self.context.profile.clone(),
                threads: self.context.encoder_threads.max(1),
            },
            Some(progress),
            Some(self.cancel_flag.clone()),
        )
        .await;
        poll_task.abort();

        match encode_result {
            Ok(()) => {}
            Err(EncodeError::Cancelled) => return Err(self.fail("Cancelled")),
            Err(EncodeError::Failed(e)) => {
                return Err(self.fail(format!("FLAC encoding failed: {e}")))
            }
        }
        self.check_cancel()?;

        if !non_flac_prefix.is_empty() {
            prepend_bytes_to_file(&self.temp_path, &non_flac_prefix)
                .map_err(|e| self.fail(format!("Failed to re-attach prefix bytes: {e}")))?;
        }

        // ── STAGE 3: ARTWORK ─────────────────────────────────────────────
        // Optimize embedded images and strip padding on the assembled temp.
        // Runs BEFORE the post-hash so verification covers the final output.
        self.stage(PipelineStage::Artwork).await;
        let artwork_result = optimize_album_art(&self.temp_path).await.ok();
        self.check_cancel()?;

        // ── STAGE 4: POST-HASH ───────────────────────────────────────────
        self.stage(PipelineStage::Hashing(HashPhase::Output)).await;
        let output_hash = hash_decoded_audio(&self.temp_path)
            .await
            .map_err(|e| self.fail(format!("Output hash failed: {e}")))?;
        self.emit(PipelineEvent::WorkerHashComputed {
            worker_id: self.worker_id,
            phase: HashPhase::Output,
            hash: output_hash.clone(),
            embedded_md5: None,
        })
        .await;

        let verification = verify_hashes(
            source_hash.as_deref(),
            Some(&output_hash),
            embedded_md5.as_deref(),
        );
        if !verification.passed {
            return Err(JobResult {
                source_hash,
                output_hash: Some(output_hash),
                embedded_md5,
                verification: verification.description,
                ..self.fail("Hash verification failed")
            });
        }

        // Everything that is not audio must be byte-for-byte the same data
        // (tags, pictures as decoded images + their metadata, application
        // and cuesheet blocks, ID3 prefix). Proven, not assumed.
        if let Err(what) = crate::flac::verify::verify_metadata_equivalent(
            file_path,
            &self.temp_path,
            self.context.mark_as_crunched,
        ) {
            return Err(JobResult {
                source_hash,
                output_hash: Some(output_hash),
                embedded_md5,
                verification: "FAIL|METADATA".to_string(),
                ..self.fail(format!("Metadata verification failed: {what}"))
            });
        }
        self.check_cancel()?;

        // ── STAGE 5: FINALIZING ──────────────────────────────────────────
        self.stage(PipelineStage::Finalizing).await;
        let after_size = std::fs::metadata(&self.temp_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // A re-encode that is not smaller is not an improvement: keep the
        // original byte-for-byte (sources already at maximum compression, or
        // encoded by a newer libFLAC, routinely come out a little larger).
        if after_size >= before_size {
            safe_remove(&self.temp_path);
            return Ok(JobResult {
                status: FileStatus::OK,
                file_path: file_path.to_string_lossy().to_string(),
                attempt: self.item.attempt,
                before_size,
                after_size: before_size,
                saved_bytes: 0,
                compression_pct: 0.0,
                source_hash,
                output_hash: Some(output_hash),
                embedded_md5,
                verification: format!("{}|KEPT", verification.description),
                artwork_result: None,
                error: Some(format!(
                    "Re-encode would be {} larger; original kept",
                    crate::util::format::format_bytes(
                        after_size as i64 - before_size as i64,
                        false
                    )
                )),
            });
        }

        safe_move(&self.temp_path, file_path)
            .map_err(|e| self.fail(format!("Failed to replace original file: {e}")))?;
        if let Some(snapshot) = &metadata_snapshot {
            let _ = restore_metadata(file_path, snapshot);
        }

        let saved_bytes = before_size as i64 - after_size as i64;
        let compression_pct = if before_size > 0 {
            (saved_bytes as f64 / before_size as f64) * 100.0
        } else {
            0.0
        };

        Ok(JobResult {
            status: FileStatus::OK,
            file_path: file_path.to_string_lossy().to_string(),
            attempt: self.item.attempt,
            before_size,
            after_size,
            saved_bytes,
            compression_pct,
            source_hash,
            output_hash: Some(output_hash),
            embedded_md5,
            verification: verification.description,
            artwork_result,
            error: None,
        })
    }
}

/// Decide whether the re-encoded output is acceptable.
///
/// * `MATCH`      — PRE == OUT and the source carried an embedded MD5.
/// * `MATCH|NEW`  — PRE == OUT; source had no/null embedded MD5.
/// * `MATCH|EMB`  — PRE unavailable, but OUT equals the embedded MD5.
/// * `FAIL|…`     — anything else.
pub fn verify_hashes(
    source_hash: Option<&str>,
    output_hash: Option<&str>,
    embedded_md5: Option<&str>,
) -> VerificationResult {
    let embedded_real = embedded_md5.filter(|e| *e != NULL_MD5);
    match (source_hash, output_hash) {
        (Some(src), Some(out)) if src == out => VerificationResult {
            passed: true,
            description: if embedded_real.is_some() {
                "MATCH".to_string()
            } else {
                "MATCH|NEW".to_string()
            },
        },
        (Some(src), Some(out)) => VerificationResult {
            passed: false,
            description: format!(
                "FAIL|MISMATCH src={} out={}",
                &src[..8.min(src.len())],
                &out[..8.min(out.len())]
            ),
        },
        (None, Some(out)) => match embedded_real {
            Some(emb) if emb == out => VerificationResult {
                passed: true,
                description: "MATCH|EMB".to_string(),
            },
            _ => VerificationResult {
                passed: false,
                description: "FAIL|NO_SRC".to_string(),
            },
        },
        _ => VerificationResult {
            passed: false,
            description: "FAIL|NO_HASH".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn test_verify_match_with_embedded() {
        let v = verify_hashes(Some(A), Some(A), Some(A));
        assert!(v.passed);
        assert_eq!(v.description, "MATCH");
    }

    #[test]
    fn test_verify_match_new_when_embedded_null_or_missing() {
        assert_eq!(
            verify_hashes(Some(A), Some(A), Some(NULL_MD5)).description,
            "MATCH|NEW"
        );
        assert_eq!(
            verify_hashes(Some(A), Some(A), None).description,
            "MATCH|NEW"
        );
    }

    #[test]
    fn test_verify_mismatch_fails_with_prefixes() {
        let v = verify_hashes(Some(A), Some(B), Some(A));
        assert!(!v.passed);
        assert_eq!(v.description, "FAIL|MISMATCH src=aaaaaaaa out=bbbbbbbb");
    }

    #[test]
    fn test_verify_falls_back_to_embedded_when_source_missing() {
        let v = verify_hashes(None, Some(A), Some(A));
        assert!(v.passed);
        assert_eq!(v.description, "MATCH|EMB");

        let v = verify_hashes(None, Some(A), Some(B));
        assert!(!v.passed);
        assert_eq!(v.description, "FAIL|NO_SRC");

        let v = verify_hashes(None, Some(A), Some(NULL_MD5));
        assert!(!v.passed);
        assert_eq!(v.description, "FAIL|NO_SRC");
    }

    #[test]
    fn test_verify_no_output_hash_fails() {
        assert_eq!(
            verify_hashes(Some(A), None, None).description,
            "FAIL|NO_HASH"
        );
        assert_eq!(verify_hashes(None, None, None).description, "FAIL|NO_HASH");
    }

    /// Full pipeline on a copy of the fixture: must succeed, shrink the file,
    /// verify, and leave no temp file behind.
    #[tokio::test]
    async fn test_execute_job_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let copy = crate::test_util::small_flac(dir.path(), "job.flac", 2);
        crate::test_util::plant_illegal_comment(&copy, b"COMMENT=\xff not utf-8");
        let size = std::fs::metadata(&copy).unwrap().len();

        let item = QueueItem {
            file: crate::fs::scanner::ScannedFile {
                path: copy.clone(),
                name: "job.flac".into(),
                size,
            },
            attempt: 1,
        };
        let ctx = ProcessingContext {
            max_retries: 0,
            mark_as_crunched: true,
            skip_crunched: true,
            profile: CompressionProfile::default(),
            encoder_threads: 1,
        };
        let (tx, mut rx) = mpsc::channel(1024);
        let result = execute_job(&item, 0, &ctx, &tx, CancellationToken::new()).await;
        drop(tx);

        assert_eq!(result.status, FileStatus::OK, "{:?}", result.error);
        assert!(
            result.verification.starts_with("MATCH"),
            "{}",
            result.verification
        );
        assert_eq!(result.source_hash, result.output_hash);
        assert!(result.after_size > 0 && result.after_size <= size);
        assert!(!temp_path_for(&copy).exists());
        assert!(marker::read_crunched_marker(&copy).unwrap());

        // Stage events arrive in pipeline order.
        let mut stages = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                PipelineEvent::WorkerStarted { stage, .. } => stages.push(stage),
                PipelineEvent::WorkerStageChanged { stage, .. } => stages.push(stage),
                _ => {}
            }
        }
        assert_eq!(
            stages,
            vec![
                PipelineStage::Hashing(HashPhase::Source),
                PipelineStage::Converting,
                PipelineStage::Artwork,
                PipelineStage::Hashing(HashPhase::Output),
                PipelineStage::Finalizing,
            ]
        );

        // Second run without skip: the re-encode cannot beat the first pass,
        // so the original must be kept byte-for-byte and still count as OK.
        let bytes_after_first = std::fs::read(&copy).unwrap();
        let item_again = QueueItem {
            file: crate::fs::scanner::ScannedFile {
                path: copy.clone(),
                name: "job.flac".into(),
                size: result.after_size,
            },
            attempt: 1,
        };
        let ctx_no_skip = ProcessingContext {
            max_retries: 0,
            mark_as_crunched: false,
            skip_crunched: false,
            profile: CompressionProfile::default(),
            encoder_threads: 1,
        };
        let (tx, _rx) = mpsc::channel(64);
        let again = execute_job(&item_again, 0, &ctx_no_skip, &tx, CancellationToken::new()).await;
        assert_eq!(again.status, FileStatus::OK, "{:?}", again.error);
        assert!(
            again.verification.ends_with("|KEPT"),
            "{}",
            again.verification
        );
        assert_eq!(again.saved_bytes, 0);
        assert_eq!(
            std::fs::read(&copy).unwrap(),
            bytes_after_first,
            "original untouched"
        );
        assert!(!temp_path_for(&copy).exists());

        // Second run with skip_crunched: the marker short-circuits the job.
        let item2 = QueueItem {
            file: crate::fs::scanner::ScannedFile {
                path: copy.clone(),
                name: "job.flac".into(),
                size: result.after_size,
            },
            attempt: 1,
        };
        let (tx, _rx) = mpsc::channel(64);
        let skipped = execute_job(&item2, 0, &ctx, &tx, CancellationToken::new()).await;
        assert_eq!(skipped.status, FileStatus::SKIPPED);
    }

    #[tokio::test]
    async fn test_execute_job_missing_file_fails_cleanly() {
        let item = QueueItem {
            file: crate::fs::scanner::ScannedFile {
                path: PathBuf::from("/nope/missing.flac"),
                name: "missing.flac".into(),
                size: 1,
            },
            attempt: 1,
        };
        let ctx = ProcessingContext {
            max_retries: 0,
            mark_as_crunched: false,
            skip_crunched: false,
            profile: CompressionProfile::default(),
            encoder_threads: 1,
        };
        let (tx, _rx) = mpsc::channel(64);
        let result = execute_job(&item, 0, &ctx, &tx, CancellationToken::new()).await;
        assert_eq!(result.status, FileStatus::FAIL);
        assert!(result.error.unwrap().contains("FLAC encoding failed"));
    }
}
