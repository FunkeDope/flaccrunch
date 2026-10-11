//! Streaming FLAC re-encoder built on libFLAC.
//!
//! The source file is decoded frame by frame and each frame is fed straight
//! into the encoder, so memory use is bounded by one block regardless of
//! track length.
//!
//! Metadata handling is deliberately split in two:
//!
//! * The encoder itself only gets a SEEKTABLE template (it fills in the byte
//!   offsets while encoding). Nothing else is attached, because libFLAC
//!   *validates* attached VORBIS_COMMENT / PICTURE blocks at init and rejects
//!   files whose tags were written by sloppy taggers (non-UTF-8 values, odd
//!   names, duplicate blocks…). It would also overwrite the vendor string.
//! * Tags, pictures, application and cuesheet blocks are cloned from the
//!   source and spliced into the output afterwards with the chain API, which
//!   validates nothing and keeps every byte as it was.

use crate::flac::ffi::{
    decoder_error_status_string, decoder_init_status_string, decoder_state_string,
    encoder_init_status_string, encoder_state_string, path_cstring,
};
use crate::flac::marker;
use crate::flac::metadata::{apply_metadata_blocks, clone_metadata_blocks, MetadataBlocks};
use libflac_sys::*;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

/// Seek point spacing, in seconds (the `flac` CLI default of `-S 10s`).
const SEEKPOINT_SPACING_SECS: u32 = 10;

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
/// pictures, application and cuesheet blocks carried over verbatim and a
/// fresh 10-second seek table.
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
    seektable: *mut FLAC__StreamMetadata,
}

impl Drop for Handles {
    fn drop(&mut self) {
        unsafe {
            // Encoder first: it may still reference the seek table.
            if !self.encoder.is_null() {
                FLAC__stream_encoder_delete(self.encoder);
            }
            if !self.decoder.is_null() {
                FLAC__stream_decoder_delete(self.decoder);
            }
            if !self.seektable.is_null() {
                FLAC__metadata_object_delete(self.seektable);
            }
        }
    }
}

/// Build a SEEKTABLE template with one point every `SEEKPOINT_SPACING_SECS`.
/// The encoder fills in the offsets. Returns null when the length is unknown.
unsafe fn seektable_template(sample_rate: u32, total_samples: u64) -> *mut FLAC__StreamMetadata {
    if total_samples == 0 || sample_rate == 0 {
        return std::ptr::null_mut();
    }
    let block = FLAC__metadata_object_new(FLAC__METADATA_TYPE_SEEKTABLE);
    if block.is_null() {
        return block;
    }
    let spacing = sample_rate.saturating_mul(SEEKPOINT_SPACING_SECS);
    if FLAC__metadata_object_seektable_template_append_spaced_points_by_samples(
        block,
        spacing,
        total_samples,
    ) == 0
        || FLAC__metadata_object_seektable_template_sort(block, 1) == 0
    {
        FLAC__metadata_object_delete(block);
        return std::ptr::null_mut();
    }
    block
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
            seektable: std::ptr::null_mut(),
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

        // Only a seek table is attached (see module docs for why not the rest).
        handles.seektable = seektable_template(state.sample_rate, state.total_samples);
        if !handles.seektable.is_null() {
            let mut list = [handles.seektable];
            if FLAC__stream_encoder_set_metadata(enc, list.as_mut_ptr(), 1) == 0 {
                return Err(fail("Failed to attach seek table to encoder".to_string()));
            }
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
        // MD5 of the decoded audio) and the seek table, and completes
        // verification.
        if FLAC__stream_encoder_finish(enc) == 0 {
            return Err(fail(format!(
                "FLAC encoder finish failed: {}",
                encoder_state_string(FLAC__stream_encoder_get_state(enc))
            )));
        }
        FLAC__stream_decoder_finish(handles.decoder);
    }

    // Tags / pictures / application / cuesheet: spliced in verbatim.
    apply_metadata_blocks(output, blocks)
        .map_err(|e| fail(format!("Failed to copy metadata to output: {e}")))?;

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
    use crate::flac::metadata::{get_md5sum, read_vorbis_comments, MetadataChain};
    use crate::test_util::{block_types, plant_illegal_comment, small_flac};

    fn fixture() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        p.exists().then_some(p)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
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
        let err = rt()
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
        let err = rt()
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
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 3);
        let out = dir.path().join("out.flac");
        let cancel = Arc::new(AtomicBool::new(true));
        let err = rt()
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

    /// Small synthetic file: audio identical, tags byte-identical (vendor
    /// string included), exactly one VORBIS_COMMENT, pictures in place, seek
    /// table added, no padding, marker written.
    #[test]
    fn test_encode_small_roundtrip_is_byte_exact_for_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 2);
        let out = dir.path().join("out.flac");
        let progress = Arc::new(EncodeProgress::new());
        rt().block_on(encode_flac(
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

        let src_hash = rt().block_on(hash_decoded_audio(&src)).unwrap();
        let out_hash = rt().block_on(hash_decoded_audio(&out)).unwrap();
        assert_eq!(src_hash, out_hash);
        assert_eq!(rt().block_on(get_md5sum(&out)).unwrap(), out_hash);

        let src_vc = read_vorbis_comments(&src).unwrap();
        let mut out_vc = read_vorbis_comments(&out).unwrap();
        assert_eq!(out_vc.len(), 1, "exactly one VORBIS_COMMENT in the output");
        assert_eq!(
            out_vc[0].vendor, src_vc[0].vendor,
            "vendor string preserved"
        );
        // Only the marker entry was added, at the end.
        let marker_entry = out_vc[0].entries.pop().unwrap();
        assert!(marker_entry.starts_with(b"FLACCRUNCH_INFO="));
        assert_eq!(out_vc[0].entries, src_vc[0].entries, "tags byte-identical");
        assert!(read_crunched_marker(&out).unwrap());

        let types = block_types(&out);
        assert_eq!(types[0], FLAC__METADATA_TYPE_STREAMINFO);
        assert!(types.contains(&FLAC__METADATA_TYPE_SEEKTABLE), "{types:?}");
        assert_eq!(
            types
                .iter()
                .filter(|&&t| t == FLAC__METADATA_TYPE_PICTURE)
                .count(),
            types.len()
                - types
                    .iter()
                    .filter(|&&t| t != FLAC__METADATA_TYPE_PICTURE)
                    .count()
        );
        assert!(!types.contains(&FLAC__METADATA_TYPE_PADDING));
        // VORBIS_COMMENT sits before the pictures, as in the source.
        let vc_pos = types
            .iter()
            .position(|&t| t == FLAC__METADATA_TYPE_VORBIS_COMMENT)
            .unwrap();
        let pic_pos = types
            .iter()
            .position(|&t| t == FLAC__METADATA_TYPE_PICTURE)
            .unwrap();
        assert!(vc_pos < pic_pos);

        // Seek table is well-formed.
        let chain = MetadataChain::read(&out).unwrap();
        let it = chain.iter().unwrap();
        let st = it.find(FLAC__METADATA_TYPE_SEEKTABLE).unwrap();
        assert_ne!(unsafe { FLAC__metadata_object_seektable_is_legal(st) }, 0);
    }

    /// The regression that broke v1.2.0: tags libFLAC's encoder would reject
    /// (non-UTF-8 value, name with a space) must be carried over verbatim.
    #[test]
    fn test_encode_tolerates_illegal_tags() {
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 1);
        plant_illegal_comment(&src, b"ARTIST=\xff\xfe latin-1 \xe9");
        plant_illegal_comment(&src, b"BAD NAME=value");
        plant_illegal_comment(&src, b"noequalsign");
        let src_vc = read_vorbis_comments(&src).unwrap();
        assert!(src_vc[0]
            .entries
            .iter()
            .any(|e| e.starts_with(b"ARTIST=\xff")));

        let out = dir.path().join("out.flac");
        rt().block_on(encode_flac(
            &src,
            &out,
            EncodeOptions::default(),
            None,
            None,
        ))
        .expect("encode must succeed despite illegal tags");
        let out_vc = read_vorbis_comments(&out).unwrap();
        assert_eq!(out_vc, src_vc, "every tag byte must survive");
        assert_eq!(
            rt().block_on(hash_decoded_audio(&src)).unwrap(),
            rt().block_on(hash_decoded_audio(&out)).unwrap()
        );
    }

    /// Full-size fixture: the real cover art and tags survive the trip.
    #[test]
    fn test_encode_fixture_roundtrip_preserves_audio_and_metadata() {
        let Some(src) = fixture() else {
            eprintln!("fixture missing — skipping");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.flac");
        rt().block_on(encode_flac(
            &src,
            &out,
            EncodeOptions::default(),
            None,
            None,
        ))
        .expect("encode");

        assert_eq!(
            rt().block_on(hash_decoded_audio(&src)).unwrap(),
            rt().block_on(hash_decoded_audio(&out)).unwrap()
        );
        assert_eq!(
            read_vorbis_comments(&src).unwrap(),
            read_vorbis_comments(&out).unwrap()
        );
        let src_pics = block_types(&src)
            .iter()
            .filter(|&&t| t == FLAC__METADATA_TYPE_PICTURE)
            .count();
        let out_pics = block_types(&out)
            .iter()
            .filter(|&&t| t == FLAC__METADATA_TYPE_PICTURE)
            .count();
        assert_eq!(src_pics, out_pics);
        assert!(crate::flac::verify::verify_metadata_equivalent(&src, &out, false).is_ok());
    }
}
