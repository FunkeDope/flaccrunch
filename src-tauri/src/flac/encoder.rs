//! Streaming FLAC re-encoder built on libFLAC.
//!
//! The source file is decoded frame by frame and each frame is fed straight
//! into the encoder, so memory use is bounded by one block regardless of
//! track length (the previous implementation materialised the whole track as
//! `Vec<i32>`: ~4× the PCM size per worker).
//!
//! Tags, artwork, application and cuesheet blocks are cloned from the source
//! and handed to the encoder up front, so the output is written in a single
//! pass with its metadata in place — no second rewrite of the file.

use crate::flac::ffi::{
    decoder_error_status_string, decoder_init_status_string, decoder_state_string,
    encoder_init_status_string, encoder_state_string, path_cstring,
};
use crate::flac::marker;
use crate::flac::metadata::{clone_metadata_blocks, MetadataBlocks};
use libflac_sys::*;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

/// Shared, lock-free progress state updated by the encoder thread and polled
/// by whoever reports progress.
#[derive(Debug, Default)]
pub struct EncodeProgress {
    percent: AtomicU8,
    ratio_bits: AtomicU32,
}

impl EncodeProgress {
    pub fn new() -> Self {
        Self {
            percent: AtomicU8::new(0),
            ratio_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub fn percent(&self) -> u8 {
        self.percent.load(Ordering::Relaxed)
    }

    /// Output bytes written ÷ PCM bytes consumed so far.
    pub fn ratio(&self) -> f32 {
        f32::from_bits(self.ratio_bits.load(Ordering::Relaxed))
    }

    fn set(&self, percent: u8, ratio: f32) {
        self.percent.store(percent, Ordering::Relaxed);
        self.ratio_bits.store(ratio.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    /// The cancel flag was raised mid-encode. The partial output is left for
    /// the caller to remove.
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::Cancelled => write!(f, "cancelled"),
            EncodeError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeOptions {
    /// Stamp the output with a FLACCRUNCH_INFO Vorbis comment.
    pub mark_as_crunched: bool,
}

/// Re-encode `input` to `output` at maximum compression using libFLAC.
/// Equivalent to: `flac -8 -e -p -V -o <output> -- <input>` with all tags,
/// pictures, application and cuesheet blocks carried over.
pub async fn encode_flac(
    input: &Path,
    output: &Path,
    options: EncodeOptions,
    progress: Option<Arc<EncodeProgress>>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<(), EncodeError> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    tokio::task::spawn_blocking(move || {
        encode_flac_native(&input, &output, options, progress, cancel)
    })
    .await
    .map_err(|e| EncodeError::Failed(format!("Encoding task panicked: {e}")))?
}

/// Everything the decoder callbacks need.
struct StreamState {
    encoder: *mut FLAC__StreamEncoder,
    // STREAMINFO of the source, filled by the metadata callback.
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    total_samples: u64,
    // Progress bookkeeping.
    samples_done: u64,
    last_percent: u8,
    output_path: PathBuf,
    progress: Option<Arc<EncodeProgress>>,
    cancel: Option<Arc<AtomicBool>>,
    // Outcome.
    cancelled: bool,
    error: Option<String>,
}

impl StreamState {
    fn bytes_per_frame(&self) -> u64 {
        self.channels as u64 * self.bits_per_sample.div_ceil(8) as u64
    }

    fn report_progress(&mut self) {
        let Some(progress) = &self.progress else {
            return;
        };
        if self.total_samples == 0 {
            return;
        }
        let percent =
            ((self.samples_done as f64 / self.total_samples as f64) * 100.0).min(100.0) as u8;
        if percent == self.last_percent {
            return;
        }
        self.last_percent = percent;
        let pcm_consumed = self.samples_done * self.bytes_per_frame();
        let out_size = std::fs::metadata(&self.output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let ratio = if pcm_consumed > 0 {
            out_size as f32 / pcm_consumed as f32
        } else {
            1.0
        };
        progress.set(percent, ratio);
    }
}

unsafe extern "C" fn write_callback(
    _decoder: *const FLAC__StreamDecoder,
    frame: *const FLAC__Frame,
    buffer: *const *const FLAC__int32,
    client_data: *mut c_void,
) -> FLAC__StreamDecoderWriteStatus {
    let state = &mut *(client_data as *mut StreamState);
    let blocksize = (*frame).header.blocksize;

    if state
        .cancel
        .as_ref()
        .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        state.cancelled = true;
        return FLAC__STREAM_DECODER_WRITE_STATUS_ABORT;
    }

    if FLAC__stream_encoder_process(state.encoder, buffer, blocksize) == 0 {
        let enc_state = FLAC__stream_encoder_get_state(state.encoder);
        let mut msg = format!(
            "Encoding failed at sample {}: {}",
            state.samples_done,
            encoder_state_string(enc_state)
        );
        if enc_state == FLAC__STREAM_ENCODER_VERIFY_DECODER_ERROR {
            let vs = FLAC__stream_encoder_get_verify_decoder_state(state.encoder);
            msg.push_str(&format!(" (verify decoder: {})", decoder_state_string(vs)));
        }
        state.error = Some(msg);
        return FLAC__STREAM_DECODER_WRITE_STATUS_ABORT;
    }

    state.samples_done += blocksize as u64;
    state.report_progress();
    FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
}

unsafe extern "C" fn metadata_callback(
    _decoder: *const FLAC__StreamDecoder,
    metadata: *const FLAC__StreamMetadata,
    client_data: *mut c_void,
) {
    let state = &mut *(client_data as *mut StreamState);
    if (*metadata).type_ == FLAC__METADATA_TYPE_STREAMINFO {
        let si = &(*metadata).data.stream_info;
        state.sample_rate = si.sample_rate;
        state.channels = si.channels;
        state.bits_per_sample = si.bits_per_sample;
        state.total_samples = si.total_samples;
    }
}

unsafe extern "C" fn error_callback(
    _decoder: *const FLAC__StreamDecoder,
    status: FLAC__StreamDecoderErrorStatus,
    client_data: *mut c_void,
) {
    let state = &mut *(client_data as *mut StreamState);
    // Keep the first error; libFLAC may report several for one bad region.
    if state.error.is_none() {
        state.error = Some(format!(
            "FLAC decode error: {}",
            decoder_error_status_string(status)
        ));
    }
}

/// RAII guard so every exit path releases the libFLAC objects.
struct Handles {
    decoder: *mut FLAC__StreamDecoder,
    encoder: *mut FLAC__StreamEncoder,
}

impl Drop for Handles {
    fn drop(&mut self) {
        unsafe {
            if !self.encoder.is_null() {
                FLAC__stream_encoder_delete(self.encoder);
            }
            if !self.decoder.is_null() {
                FLAC__stream_decoder_delete(self.decoder);
            }
        }
    }
}

fn encode_flac_native(
    input: &Path,
    output: &Path,
    options: EncodeOptions,
    progress: Option<Arc<EncodeProgress>>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<(), EncodeError> {
    let fail = |msg: String| EncodeError::Failed(msg);

    // Metadata to carry over, cloned before we touch the output.
    let mut blocks: MetadataBlocks = clone_metadata_blocks(input).map_err(fail)?;
    if options.mark_as_crunched {
        marker::ensure_marker_in_blocks(&mut blocks).map_err(fail)?;
    }

    let input_c = path_cstring(input).map_err(fail)?;
    let output_c = path_cstring(output).map_err(fail)?;

    let mut state = StreamState {
        encoder: std::ptr::null_mut(),
        sample_rate: 0,
        channels: 0,
        bits_per_sample: 0,
        total_samples: 0,
        samples_done: 0,
        last_percent: 0,
        output_path: output.to_path_buf(),
        progress,
        cancel,
        cancelled: false,
        error: None,
    };

    unsafe {
        let mut handles = Handles {
            decoder: FLAC__stream_decoder_new(),
            encoder: std::ptr::null_mut(),
        };
        if handles.decoder.is_null() {
            return Err(fail("Failed to create FLAC decoder".to_string()));
        }

        let init = FLAC__stream_decoder_init_file(
            handles.decoder,
            input_c.as_ptr(),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            &mut state as *mut _ as *mut c_void,
        );
        if init != FLAC__STREAM_DECODER_INIT_STATUS_OK {
            return Err(fail(format!(
                "Failed to init FLAC decoder: {}",
                decoder_init_status_string(init)
            )));
        }

        // Read STREAMINFO so the encoder can be configured before any audio.
        if FLAC__stream_decoder_process_until_end_of_metadata(handles.decoder) == 0 {
            return Err(fail(state.error.take().unwrap_or_else(|| {
                format!(
                    "Failed to read FLAC metadata: {}",
                    decoder_state_string(FLAC__stream_decoder_get_state(handles.decoder))
                )
            })));
        }
        if state.sample_rate == 0 || state.channels == 0 {
            return Err(fail("No STREAMINFO found in FLAC file".to_string()));
        }

        handles.encoder = FLAC__stream_encoder_new();
        if handles.encoder.is_null() {
            return Err(fail("Failed to create FLAC encoder".to_string()));
        }
        let enc = handles.encoder;

        // flac -8 -e -p -V
        FLAC__stream_encoder_set_channels(enc, state.channels);
        FLAC__stream_encoder_set_bits_per_sample(enc, state.bits_per_sample);
        FLAC__stream_encoder_set_sample_rate(enc, state.sample_rate);
        FLAC__stream_encoder_set_compression_level(enc, 8);
        FLAC__stream_encoder_set_do_exhaustive_model_search(enc, 1);
        FLAC__stream_encoder_set_do_qlp_coeff_prec_search(enc, 1);
        FLAC__stream_encoder_set_verify(enc, 1);
        if state.total_samples > 0 {
            FLAC__stream_encoder_set_total_samples_estimate(enc, state.total_samples);
        }
        if !blocks.is_empty()
            && FLAC__stream_encoder_set_metadata(enc, blocks.as_mut_ptr(), blocks.len() as u32) == 0
        {
            return Err(fail("Failed to attach metadata to encoder".to_string()));
        }

        let init =
            FLAC__stream_encoder_init_file(enc, output_c.as_ptr(), None, std::ptr::null_mut());
        if init != FLAC__STREAM_ENCODER_INIT_STATUS_OK {
            let mut msg = format!(
                "Failed to init FLAC encoder: {}",
                encoder_init_status_string(init)
            );
            if init == FLAC__STREAM_ENCODER_INIT_STATUS_ENCODER_ERROR {
                msg.push_str(&format!(
                    " ({})",
                    encoder_state_string(FLAC__stream_encoder_get_state(enc))
                ));
            }
            return Err(fail(msg));
        }
        state.encoder = enc;

        let ok = FLAC__stream_decoder_process_until_end_of_stream(handles.decoder);

        if state.cancelled {
            // Deliberately skip encoder_finish: the partial temp file is
            // removed by the caller.
            return Err(EncodeError::Cancelled);
        }
        if let Some(err) = state.error.take() {
            return Err(fail(err));
        }
        if ok == 0 {
            return Err(fail(format!(
                "FLAC decoding failed: {}",
                decoder_state_string(FLAC__stream_decoder_get_state(handles.decoder))
            )));
        }

        // finish() flushes the last frame, back-patches STREAMINFO (incl. the
        // MD5 of the decoded audio) and completes verification.
        if FLAC__stream_encoder_finish(enc) == 0 {
            return Err(fail(format!(
                "FLAC encoder finish failed: {}",
                encoder_state_string(FLAC__stream_encoder_get_state(enc))
            )));
        }
        FLAC__stream_decoder_finish(handles.decoder);
        // `handles` and `blocks` drop here; the encoder has already written
        // the metadata so freeing the block objects is safe.
    }

    if let Some(p) = &state.progress {
        p.set(100, p.ratio());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flac::hasher::hash_decoded_audio;
    use crate::flac::marker::read_crunched_marker;
    use crate::flac::metadata::get_md5sum;

    fn fixture() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        p.exists().then_some(p)
    }

    #[test]
    fn test_encode_progress_defaults() {
        let p = EncodeProgress::new();
        assert_eq!(p.percent(), 0);
        assert_eq!(p.ratio(), 1.0);
        p.set(42, 0.5);
        assert_eq!(p.percent(), 42);
        assert_eq!(p.ratio(), 0.5);
    }

    #[test]
    fn test_encode_missing_input_fails() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.flac");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(encode_flac(
                Path::new("/nope/missing.flac"),
                &out,
                EncodeOptions::default(),
                None,
                None,
            ))
            .unwrap_err();
        assert!(matches!(err, EncodeError::Failed(_)), "{err}");
    }

    #[test]
    fn test_encode_rejects_non_flac_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("junk.flac");
        std::fs::write(&input, b"definitely not a flac file").unwrap();
        let out = dir.path().join("out.flac");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(encode_flac(
                &input,
                &out,
                EncodeOptions::default(),
                None,
                None,
            ))
            .unwrap_err();
        assert!(matches!(err, EncodeError::Failed(_)), "{err}");
    }

    #[test]
    fn test_encode_cancel_flag_aborts() {
        let Some(src) = fixture() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.flac");
        let cancel = Arc::new(AtomicBool::new(true));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(encode_flac(
                &src,
                &out,
                EncodeOptions::default(),
                None,
                Some(cancel),
            ))
            .unwrap_err();
        assert_eq!(err, EncodeError::Cancelled);
    }

    /// End-to-end: re-encode the fixture, verify decoded audio is identical,
    /// metadata was carried over and the marker was written.
    #[test]
    fn test_encode_fixture_roundtrip_preserves_audio_and_metadata() {
        let Some(src) = fixture() else {
            eprintln!("fixture missing — skipping");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.flac");
        let progress = Arc::new(EncodeProgress::new());
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(encode_flac(
            &src,
            &out,
            EncodeOptions {
                mark_as_crunched: true,
            },
            Some(progress.clone()),
            None,
        ))
        .expect("encode");

        assert_eq!(progress.percent(), 100);
        assert!(
            progress.ratio() > 0.0 && progress.ratio() < 1.0,
            "{}",
            progress.ratio()
        );

        let src_hash = rt.block_on(hash_decoded_audio(&src)).unwrap();
        let out_hash = rt.block_on(hash_decoded_audio(&out)).unwrap();
        assert_eq!(src_hash, out_hash, "decoded audio must be bit-identical");
        assert_eq!(rt.block_on(get_md5sum(&out)).unwrap(), out_hash);

        // Metadata carried over in a single pass.
        let src_blocks = clone_metadata_blocks(&src).unwrap();
        let out_blocks = clone_metadata_blocks(&out).unwrap();
        assert_eq!(src_blocks.len(), out_blocks.len());
        assert!(out_blocks.find(FLAC__METADATA_TYPE_PICTURE).is_some());
        assert!(read_crunched_marker(&out).unwrap());
        assert!(!read_crunched_marker(&src).unwrap());

        // Output carries no PADDING at all.
        let chain = crate::flac::metadata::MetadataChain::read(&out).unwrap();
        let iter = chain.iter().unwrap();
        assert!(iter.find(FLAC__METADATA_TYPE_PADDING).is_none());
    }
}
