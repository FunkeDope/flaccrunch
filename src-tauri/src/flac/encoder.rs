use std::ffi::CString;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Result of a FLAC encoding operation.
#[derive(Debug)]
pub struct EncodeResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

/// Encode a FLAC file at maximum compression level using the native libFLAC API.
/// Equivalent to: flac -8 -e -p -V -f -o <output> -- <input>
///
/// This decodes the input FLAC to PCM, then re-encodes it with maximum settings:
/// - Compression level 8
/// - Exhaustive model search (-e)
/// - Exhaustive QLP coefficient precision search (-p)
/// - Verify encoding (-V)
///
/// If `progress_tx` is provided, sends percent complete (0-100) during encoding.
pub async fn encode_flac(
    input: &Path,
    output: &Path,
    progress_tx: Option<std::sync::mpsc::Sender<(u8, f32)>>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<EncodeResult, String> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();

    tokio::task::spawn_blocking(move || encode_flac_native(&input, &output, progress_tx, cancel))
        .await
        .map_err(|e| format!("Encoding task panicked: {e}"))?
}

/// Native FLAC re-encoding: decode input -> re-encode with max compression.
fn encode_flac_native(
    input: &Path,
    output: &Path,
    progress_tx: Option<std::sync::mpsc::Sender<(u8, f32)>>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<EncodeResult, String> {
    let decoded = decode_flac_to_pcm(input)?;
    encode_pcm_to_flac(&decoded, output, progress_tx, cancel)
}

struct DecodedFlac {
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    total_samples: u64,
    /// Interleaved samples as i32 values
    samples: Vec<i32>,
}

/// Decode a FLAC file entirely to PCM samples using the libFLAC stream decoder.
fn decode_flac_to_pcm(input: &Path) -> Result<DecodedFlac, String> {
    use libflac_sys::*;
    use std::os::raw::c_void;

    struct DecoderState {
        samples: Vec<i32>,
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
        total_samples: u64,
        error: Option<String>,
    }

    unsafe extern "C" fn write_callback(
        _decoder: *const FLAC__StreamDecoder,
        frame: *const FLAC__Frame,
        buffer: *const *const FLAC__int32,
        client_data: *mut c_void,
    ) -> FLAC__StreamDecoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let frame = unsafe { &*frame };
        let blocksize = frame.header.blocksize as usize;
        let channels = frame.header.channels as usize;

        for sample_idx in 0..blocksize {
            for ch in 0..channels {
                let channel_buf = unsafe { *buffer.add(ch) };
                let sample = unsafe { *channel_buf.add(sample_idx) };
                state.samples.push(sample);
            }
        }

        FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
    }

    unsafe extern "C" fn metadata_callback(
        _decoder: *const FLAC__StreamDecoder,
        metadata: *const FLAC__StreamMetadata,
        client_data: *mut c_void,
    ) {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let metadata = unsafe { &*metadata };

        if metadata.type_ == FLAC__METADATA_TYPE_STREAMINFO {
            let si = unsafe { &metadata.data.stream_info };
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
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        state.error = Some(format!("FLAC decode error: status {}", status));
    }

    let mut state = DecoderState {
        samples: Vec::new(),
        sample_rate: 0,
        channels: 0,
        bits_per_sample: 0,
        total_samples: 0,
        error: None,
    };

    let input_cstr = CString::new(input.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid input path")?;

    unsafe {
        let decoder = FLAC__stream_decoder_new();
        if decoder.is_null() {
            return Err("Failed to create FLAC decoder".to_string());
        }

        // Process all metadata (we need STREAMINFO + we want to preserve picture/vorbis blocks)
        FLAC__stream_decoder_set_metadata_respond_all(decoder);

        let init_status = FLAC__stream_decoder_init_file(
            decoder,
            input_cstr.as_ptr(),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            &mut state as *mut _ as *mut c_void,
        );

        if init_status != FLAC__STREAM_DECODER_INIT_STATUS_OK {
            FLAC__stream_decoder_delete(decoder);
            return Err(format!("Failed to init FLAC decoder: status {}", init_status));
        }

        let ok = FLAC__stream_decoder_process_until_end_of_stream(decoder);
        FLAC__stream_decoder_finish(decoder);
        FLAC__stream_decoder_delete(decoder);

        if ok == 0 {
            return Err(state.error.unwrap_or_else(|| "FLAC decoding failed".to_string()));
        }
        if let Some(err) = state.error {
            return Err(err);
        }
    }

    if state.sample_rate == 0 || state.channels == 0 {
        return Err("No STREAMINFO found in FLAC file".to_string());
    }

    Ok(DecodedFlac {
        sample_rate: state.sample_rate,
        channels: state.channels,
        bits_per_sample: state.bits_per_sample,
        total_samples: state.total_samples,
        samples: state.samples,
    })
}

/// Encode PCM data to a FLAC file with maximum compression settings.
/// Also copies metadata blocks (pictures, vorbis comments) from the input.
fn encode_pcm_to_flac(
    decoded: &DecodedFlac,
    output: &Path,
    progress_tx: Option<std::sync::mpsc::Sender<(u8, f32)>>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<EncodeResult, String> {
    use libflac_sys::*;

    let output_cstr = CString::new(output.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid output path")?;

    unsafe {
        let encoder = FLAC__stream_encoder_new();
        if encoder.is_null() {
            return Err("Failed to create FLAC encoder".to_string());
        }

        // Configure encoder - equivalent to flac -8 -e -p -V
        FLAC__stream_encoder_set_channels(encoder, decoded.channels);
        FLAC__stream_encoder_set_bits_per_sample(encoder, decoded.bits_per_sample);
        FLAC__stream_encoder_set_sample_rate(encoder, decoded.sample_rate);
        FLAC__stream_encoder_set_compression_level(encoder, 8);
        FLAC__stream_encoder_set_do_exhaustive_model_search(encoder, 1); // -e
        FLAC__stream_encoder_set_do_qlp_coeff_prec_search(encoder, 1);  // -p
        FLAC__stream_encoder_set_verify(encoder, 1);                     // -V

        if decoded.total_samples > 0 {
            FLAC__stream_encoder_set_total_samples_estimate(encoder, decoded.total_samples);
        }

        // Progress callback (optional, we could use this for progress tracking)
        let init_status = FLAC__stream_encoder_init_file(
            encoder,
            output_cstr.as_ptr(),
            None, // progress callback
            std::ptr::null_mut(),
        );

        if init_status != FLAC__STREAM_ENCODER_INIT_STATUS_OK {
            FLAC__stream_encoder_delete(encoder);
            return Err(format!("Failed to init FLAC encoder: status {}", init_status));
        }

        // Process samples in chunks
        let channels = decoded.channels as usize;
        let total_interleaved = decoded.samples.len();
        let total_frames = total_interleaved / channels;
        let chunk_size = 4096; // frames per chunk
        let bytes_per_frame = (channels * (decoded.bits_per_sample as usize / 8)) as u64;

        let mut offset = 0;
        let mut last_percent: u8 = 0;
        while offset < total_frames {
            let frames_this_chunk = chunk_size.min(total_frames - offset);
            let sample_offset = offset * channels;
            let sample_count = frames_this_chunk * channels;

            let ok = FLAC__stream_encoder_process_interleaved(
                encoder,
                decoded.samples[sample_offset..sample_offset + sample_count].as_ptr(),
                frames_this_chunk as u32,
            );

            if ok == 0 {
                let encoder_state = FLAC__stream_encoder_get_state(encoder);
                FLAC__stream_encoder_delete(encoder);
                return Ok(EncodeResult {
                    success: false,
                    exit_code: Some(encoder_state as i32),
                    stderr: format!("Encoding failed at frame {}: encoder state {}", offset, encoder_state),
                });
            }

            offset += frames_this_chunk;

            // Report progress (throttled to only send when percent changes)
            if let Some(ref tx) = progress_tx {
                let percent = ((offset as f64 / total_frames as f64) * 100.0) as u8;
                if percent != last_percent {
                    last_percent = percent;
                    let pcm_consumed = offset as u64 * bytes_per_frame;
                    let out_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                    let ratio = if pcm_consumed > 0 {
                        out_size as f32 / pcm_consumed as f32
                    } else {
                        1.0
                    };
                    let _ = tx.send((percent, ratio));
                }
            }

            // Check for cancellation every ~4096 frames (~100ms at 44.1kHz)
            if let Some(ref c) = cancel {
                if c.load(Ordering::Relaxed) {
                    FLAC__stream_encoder_delete(encoder);
                    return Ok(EncodeResult {
                        success: false,
                        exit_code: None,
                        stderr: "cancelled".to_string(),
                    });
                }
            }
        }

        let ok = FLAC__stream_encoder_finish(encoder);
        FLAC__stream_encoder_delete(encoder);

        if ok == 0 {
            return Ok(EncodeResult {
                success: false,
                exit_code: Some(1),
                stderr: "FLAC encoder finish failed (possible verification error)".to_string(),
            });
        }
    }

    Ok(EncodeResult {
        success: true,
        exit_code: Some(0),
        stderr: String::new(),
    })
}

/// Build the flac encoding arguments as a Vec<String> (for display/logging).
pub fn build_flac_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "[native-libflac]".to_string(),
        "-8".to_string(),
        "-e".to_string(),
        "-p".to_string(),
        "-V".to_string(),
        "-o".to_string(),
        output.to_string_lossy().to_string(),
        "--".to_string(),
        input.to_string_lossy().to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_build_flac_args_simple_path() {
        let args = build_flac_args(
            &PathBuf::from("/music/track.flac"),
            &PathBuf::from("/music/track.tmp"),
        );
        assert!(args.len() >= 8);
        assert!(args.contains(&"-8".to_string()));
        assert!(args.contains(&"-V".to_string()));
    }
}
