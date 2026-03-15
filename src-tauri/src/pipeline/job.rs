use crate::artwork::optimize::optimize_album_art;
use crate::flac::encoder::encode_flac;
use crate::flac::hasher::hash_decoded_audio;
use crate::flac::metadata::{copy_metadata_blocks, extract_non_flac_prefix, prepend_bytes_to_file, get_md5sum};
use crate::fs::metadata::{restore_metadata, snapshot_metadata};
use crate::fs::tempfile::{safe_move, safe_remove, temp_path_for};
use crate::pipeline::queue::QueueItem;
use crate::pipeline::stages::{JobResult, PipelineEvent, PipelineStage, VerificationResult};
use crate::state::run_state::FileStatus;
use crate::util::format::NULL_MD5;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Context shared across all jobs in a processing run.
pub struct ProcessingContext {
    pub max_retries: u32,
    pub scratch_dir: PathBuf,
}

/// Execute the full processing pipeline for a single FLAC file.
/// `cancel_token` is checked between stages and wired into the encoder loop
/// so cancellation aborts as quickly as possible.
pub async fn execute_job(
    item: &QueueItem,
    worker_id: usize,
    context: &ProcessingContext,
    event_tx: &mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
) -> JobResult {
    let file_path = &item.file.path;
    let file_name = &item.file.name;
    let temp_path = temp_path_for(file_path);

    // Shared cancel flag that the blocking encoder thread can read
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_watcher = {
        let flag = cancel_flag.clone();
        let token = cancel_token.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            flag.store(true, Ordering::Relaxed);
        })
    };

    let result = run_job(
        item,
        file_name,
        file_path,
        &temp_path,
        worker_id,
        context,
        event_tx,
        cancel_token,
        cancel_flag,
    )
    .await;

    cancel_watcher.abort();
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_job(
    item: &QueueItem,
    file_name: &str,
    file_path: &Path,
    temp_path: &Path,
    worker_id: usize,
    context: &ProcessingContext,
    event_tx: &mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
    cancel_flag: Arc<AtomicBool>,
) -> JobResult {
    let metadata_snapshot = snapshot_metadata(file_path).ok();
    let before_size = item.file.size;

    // Capture any non-FLAC prefix bytes (e.g. ID3v2 header) from the source
    // so we can re-attach them verbatim after re-encoding.
    let non_flac_prefix = extract_non_flac_prefix(file_path).unwrap_or_default();

    // =========================================================================
    // STAGE 1: PRE-HASH
    // Read the embedded MD5 (from STREAMINFO) and hash the original decoded audio
    // *before* any modification. This is the ground truth we verify against.
    // =========================================================================
    let _ = event_tx
        .send(PipelineEvent::WorkerStarted {
            worker_id,
            file: file_name.to_string(),
            stage: PipelineStage::Hashing(crate::pipeline::stages::HashPhase::Source),
        })
        .await;

    let embedded_md5 = get_md5sum(file_path).await.ok().flatten();
    let source_hash = hash_decoded_audio(file_path).await.ok();

    // Emit live hash result so the UI can display PRE + EMB immediately.
    if let Some(ref h) = source_hash {
        let _ = event_tx
            .send(PipelineEvent::WorkerHashComputed {
                worker_id,
                phase: crate::pipeline::stages::HashPhase::Source,
                hash: h.clone(),
                embedded_md5: embedded_md5.clone(),
            })
            .await;
    }

    if cancel_token.is_cancelled() {
        return make_failure(file_path, item.attempt, before_size, "Cancelled");
    }

    // =========================================================================
    // STAGE 2: CONVERTING
    // Encode audio-only to temp file (no metadata — added below).
    // =========================================================================
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Converting,
        })
        .await;

    let progress_pct = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    let pct_writer = progress_pct.clone();
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<u8>();
    std::thread::spawn(move || {
        while let Ok(pct) = sync_rx.recv() {
            pct_writer.store(pct, std::sync::atomic::Ordering::Relaxed);
        }
    });

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

    let encode_result = match encode_flac(
        file_path,
        temp_path,
        Some(sync_tx),
        Some(cancel_flag.clone()),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = done_tx.send(());
            poll_task.abort();
            safe_remove(temp_path);
            return make_failure(file_path, item.attempt, before_size, &e);
        }
    };
    let _ = done_tx.send(());
    let _ = poll_task.await;

    if encode_result.stderr == "cancelled" || cancel_token.is_cancelled() {
        safe_remove(temp_path);
        return make_failure(file_path, item.attempt, before_size, "Cancelled");
    }

    if !encode_result.success {
        safe_remove(temp_path);
        return make_failure(
            file_path,
            item.attempt,
            before_size,
            &format!(
                "FLAC encoding failed (exit code {:?}): {}",
                encode_result.exit_code, encode_result.stderr
            ),
        );
    }

    // Copy all metadata blocks (tags + artwork) from original to temp.
    // The encoder writes audio only; metadata must be re-applied separately.
    if let Err(e) = copy_metadata_blocks(file_path, temp_path).await {
        safe_remove(temp_path);
        return make_failure(
            file_path,
            item.attempt,
            before_size,
            &format!("Metadata copy failed: {e}"),
        );
    }

    // Re-attach any non-FLAC prefix bytes (ID3v2 etc.)
    if !non_flac_prefix.is_empty() {
        if let Err(e) = prepend_bytes_to_file(temp_path, &non_flac_prefix) {
            safe_remove(temp_path);
            return make_failure(
                file_path,
                item.attempt,
                before_size,
                &format!("Failed to re-attach prefix bytes: {e}"),
            );
        }
    }

    if cancel_token.is_cancelled() {
        safe_remove(temp_path);
        return make_failure(file_path, item.attempt, before_size, "Cancelled");
    }

    // =========================================================================
    // STAGE 3: ARTWORK
    // Optimize embedded images and strip padding on the fully-assembled temp.
    // Must run BEFORE the post-hash so verification covers the final output.
    // =========================================================================
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Artwork,
        })
        .await;

    let artwork_result = optimize_album_art(
        temp_path,
        &context.scratch_dir,
    )
    .await
    .ok();

    if cancel_token.is_cancelled() {
        safe_remove(temp_path);
        return make_failure(file_path, item.attempt, before_size, "Cancelled");
    }

    // =========================================================================
    // STAGE 4: POST-HASH
    // Hash the fully-finalized temp (audio + metadata + optimized artwork).
    // This verifies the complete output, not just the audio-only intermediate.
    // =========================================================================
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Hashing(crate::pipeline::stages::HashPhase::Output),
        })
        .await;

    let output_hash = match hash_decoded_audio(temp_path).await {
        Ok(h) => {
            // Emit live hash result so the UI can show OUT immediately.
            let _ = event_tx
                .send(PipelineEvent::WorkerHashComputed {
                    worker_id,
                    phase: crate::pipeline::stages::HashPhase::Output,
                    hash: h.clone(),
                    embedded_md5: None,
                })
                .await;
            Some(h)
        }
        Err(e) => {
            safe_remove(temp_path);
            return make_failure(
                file_path,
                item.attempt,
                before_size,
                &format!("Output hash failed: {e}"),
            );
        }
    };

    let verification = verify_hashes(&source_hash, &output_hash, &embedded_md5);
    if !verification.passed {
        safe_remove(temp_path);
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

    if cancel_token.is_cancelled() {
        safe_remove(temp_path);
        return make_failure(file_path, item.attempt, before_size, "Cancelled");
    }

    // =========================================================================
    // STAGE 5: FINALIZING
    // Measure final size, replace original, restore filesystem timestamps.
    // =========================================================================
    let _ = event_tx
        .send(PipelineEvent::WorkerStageChanged {
            worker_id,
            stage: PipelineStage::Finalizing,
        })
        .await;

    let after_size = std::fs::metadata(temp_path).map(|m| m.len()).unwrap_or(0);

    if let Err(e) = safe_move(temp_path, file_path) {
        safe_remove(temp_path);
        return make_failure(
            file_path,
            item.attempt,
            before_size,
            &format!("Failed to replace original file: {e}"),
        );
    }

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
            description: format!(
                "FAIL|MISMATCH src={} out={}",
                &src[..8.min(src.len())],
                &out[..8.min(out.len())]
            ),
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
