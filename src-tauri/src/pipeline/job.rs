use crate::artwork::optimize::optimize_album_art;
use crate::flac::encoder::encode_flac;
use crate::flac::hasher::hash_decoded_audio;
use crate::flac::metadata::get_md5sum;
use crate::fs::metadata::{restore_metadata, snapshot_metadata};
use crate::fs::tempfile::{safe_move, safe_remove, temp_path_for};
use crate::pipeline::queue::QueueItem;
use crate::pipeline::stages::{JobResult, PipelineEvent, PipelineStage, VerificationResult};
use crate::state::run_state::FileStatus;
use crate::util::format::NULL_MD5;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Context shared across all jobs in a processing run.
pub struct ProcessingContext {
    /// Unused on Android — native libFLAC is linked directly.
    /// On desktop, still resolved for logging purposes but not required.
    pub flac_bin: PathBuf,
    pub metaflac_bin: PathBuf,
    pub max_retries: u32,
    pub scratch_dir: PathBuf,
}

/// Execute the full processing pipeline for a single FLAC file.
pub async fn execute_job(
    item: &QueueItem,
    worker_id: usize,
    context: &ProcessingContext,
    event_tx: &mpsc::Sender<PipelineEvent>,
) -> JobResult {
    let file_path = &item.file.path;
    let file_name = &item.file.name;
    let temp_path = temp_path_for(file_path);

    // Emit start event
    let _ = event_tx
        .send(PipelineEvent::WorkerStarted {
            worker_id,
            file: file_name.clone(),
            stage: PipelineStage::Converting,
        })
        .await;

    // Capture original file metadata
    let metadata_snapshot = match snapshot_metadata(file_path) {
        Ok(s) => Some(s),
        Err(_) => None,
    };

    let before_size = item.file.size;

    // === STAGE 1: CONVERTING ===
    // Set up progress reporting: encoder writes to AtomicU8, poll task emits events
    let progress_pct = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    // Create a sync channel sender that updates the atomic from the blocking thread
    let pct_writer = progress_pct.clone();
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<u8>();
    std::thread::spawn(move || {
        while let Ok(pct) = sync_rx.recv() {
            pct_writer.store(pct, std::sync::atomic::Ordering::Relaxed);
        }
    });

    // Poll task: read atomic every 100ms, emit WorkerProgress when value changes
    let poll_event_tx = event_tx.clone();
    let poll_pct = progress_pct.clone();
    let poll_worker_id = worker_id;
    let poll_task = tokio::spawn(async move {
        let mut last = 0u8;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    let current = poll_pct.load(std::sync::atomic::Ordering::Relaxed);
                    if current != last {
                        last = current;
                        let _ = poll_event_tx.send(PipelineEvent::WorkerProgress {
                            worker_id: poll_worker_id,
                            percent: current,
                            ratio: format!("{}%", current),
                        }).await;
                    }
                }
                _ = &mut done_rx => break,
            }
        }
    });

    let encode_result = match encode_flac(&context.flac_bin, file_path, &temp_path, Some(sync_tx)).await {
        Ok(r) => r,
        Err(e) => {
            let _ = done_tx.send(());
            poll_task.abort();
            return make_failure(file_path, item.attempt, before_size, &e);
        }
    };
    let _ = done_tx.send(());
    let _ = poll_task.await;

    if !encode_result.success {
        safe_remove(&temp_path);
        return make_failure(
            file_path,
            item.attempt,
            before_size,
            &format!("FLAC encoding failed (exit code {:?}): {}", encode_result.exit_code, encode_result.stderr),
        );
    }

    // === STAGE 2: HASHING ===
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Hashing(crate::pipeline::stages::HashPhase::Source),
        })
        .await;

    // Get embedded MD5 from original file
    let embedded_md5 = get_md5sum(&context.metaflac_bin, file_path).await.ok().flatten();

    // Hash source decoded audio
    let source_hash = match hash_decoded_audio(&context.flac_bin, file_path).await {
        Ok(h) => Some(h),
        Err(_) => None,
    };

    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Hashing(crate::pipeline::stages::HashPhase::Output),
        })
        .await;

    // Hash output decoded audio
    let output_hash = match hash_decoded_audio(&context.flac_bin, &temp_path).await {
        Ok(h) => Some(h),
        Err(e) => {
            safe_remove(&temp_path);
            return make_failure(file_path, item.attempt, before_size, &format!("Output hash failed: {e}"));
        }
    };

    // Verify hashes
    let verification = verify_hashes(&source_hash, &output_hash, &embedded_md5);
    if !verification.passed {
        safe_remove(&temp_path);
        return JobResult {
            status: FileStatus::FAIL,
            file_path: file_path.to_string_lossy().to_string(),
            attempt: item.attempt,
            before_size,
            after_size: 0,
            saved_bytes: 0,
            compression_pct: 0.0,
            source_hash,
            output_hash,
            embedded_md5,
            verification: verification.description,
            artwork_result: None,
            error: Some("Hash verification failed".to_string()),
        };
    }

    // === STAGE 3: ARTWORK ===
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Artwork,
        })
        .await;

    let artwork_result = match optimize_album_art(
        &context.metaflac_bin,
        &temp_path,
        &context.scratch_dir,
    )
    .await
    {
        Ok(r) => Some(r),
        Err(_) => None,
    };

    // === STAGE 4: FINALIZING ===
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Finalizing,
        })
        .await;

    let after_size = std::fs::metadata(&temp_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Replace original with temp
    if let Err(e) = safe_move(&temp_path, file_path) {
        safe_remove(&temp_path);
        return make_failure(
            file_path,
            item.attempt,
            before_size,
            &format!("Failed to replace original file: {e}"),
        );
    }

    // Restore original metadata
    if let Some(ref snapshot) = metadata_snapshot {
        let _ = restore_metadata(file_path, snapshot);
    }

    let saved_bytes = before_size as i64 - after_size as i64;
    let compression_pct = if before_size > 0 {
        (saved_bytes as f64 / before_size as f64) * 100.0
    } else {
        0.0
    };

    JobResult {
        status: FileStatus::OK,
        file_path: file_path.to_string_lossy().to_string(),
        attempt: item.attempt,
        before_size,
        after_size,
        saved_bytes,
        compression_pct,
        source_hash,
        output_hash,
        embedded_md5,
        verification: verification.description,
        artwork_result,
        error: None,
    }
}

/// Verify that the decoded audio hashes match.
fn verify_hashes(
    source_hash: &Option<String>,
    output_hash: &Option<String>,
    embedded_md5: &Option<String>,
) -> VerificationResult {
    match (source_hash, output_hash) {
        (Some(src), Some(out)) if src == out => VerificationResult {
            passed: true,
            description: if embedded_md5.as_deref() == Some(NULL_MD5) || embedded_md5.is_none() {
                "MATCH|NEW".to_string()
            } else {
                "MATCH".to_string()
            },
            source_hash: Some(src.clone()),
            output_hash: Some(out.clone()),
            embedded_md5: embedded_md5.clone(),
        },
        (None, Some(out)) => {
            // No source hash available; check against embedded MD5
            if let Some(emb) = embedded_md5 {
                if emb != NULL_MD5 && out == emb {
                    return VerificationResult {
                        passed: true,
                        description: "MATCH|EMB".to_string(),
                        source_hash: None,
                        output_hash: Some(out.clone()),
                        embedded_md5: Some(emb.clone()),
                    };
                }
            }
            VerificationResult {
                passed: false,
                description: "FAIL|NO_SRC".to_string(),
                source_hash: None,
                output_hash: Some(out.clone()),
                embedded_md5: embedded_md5.clone(),
            }
        }
        (Some(src), Some(out)) => VerificationResult {
            passed: false,
            description: format!("FAIL|MISMATCH src={} out={}", &src[..8.min(src.len())], &out[..8.min(out.len())]),
            source_hash: Some(src.clone()),
            output_hash: Some(out.clone()),
            embedded_md5: embedded_md5.clone(),
        },
        _ => VerificationResult {
            passed: false,
            description: "FAIL|NO_HASH".to_string(),
            source_hash: source_hash.clone(),
            output_hash: output_hash.clone(),
            embedded_md5: embedded_md5.clone(),
        },
    }
}

fn make_failure(file_path: &Path, attempt: u32, before_size: u64, error: &str) -> JobResult {
    JobResult {
        status: FileStatus::FAIL,
        file_path: file_path.to_string_lossy().to_string(),
        attempt,
        before_size,
        after_size: 0,
        saved_bytes: 0,
        compression_pct: 0.0,
        source_hash: None,
        output_hash: None,
        embedded_md5: None,
        verification: "N/A".to_string(),
        artwork_result: None,
        error: Some(error.to_string()),
    }
}
