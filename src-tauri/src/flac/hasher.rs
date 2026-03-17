use md5::{Digest, Md5};
use std::ffi::CString;
use std::path::Path;

/// Compute the MD5 hash of the decoded audio stream from a FLAC file.
/// Uses native libFLAC decoder instead of spawning `flac -d -c`.
/// The hash is computed over raw PCM samples in little-endian signed format,
/// matching the output of `flac -d -c -s --force-raw-format --endian=little --sign=signed`.
pub async fn hash_decoded_audio(file_path: &Path) -> Result<String, String> {
    let file_path = file_path.to_path_buf();

    tokio::task::spawn_blocking(move || hash_decoded_audio_native(&file_path))
        .await
        .map_err(|e| format!("Hash task panicked: {e}"))?
}

/// Native implementation: decode FLAC and compute MD5 of raw PCM output.
fn hash_decoded_audio_native(file_path: &Path) -> Result<String, String> {
    use libflac_sys::*;
    use std::os::raw::c_void;

    struct HasherState {
        hasher: Md5,
        bits_per_sample: u32,
        error: Option<String>,
    }

    unsafe extern "C" fn write_callback(
        _decoder: *const FLAC__StreamDecoder,
        frame: *const FLAC__Frame,
        buffer: *const *const FLAC__int32,
        client_data: *mut c_void,
    ) -> FLAC__StreamDecoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut HasherState) };
        let frame = unsafe { &*frame };
        let blocksize = frame.header.blocksize as usize;
        let channels = frame.header.channels as usize;
        let bps = state.bits_per_sample;

        // Convert samples to little-endian signed bytes (matching flac -d --force-raw-format)
        for sample_idx in 0..blocksize {
            for ch in 0..channels {
                let channel_buf = unsafe { *buffer.add(ch) };
                let sample = unsafe { *channel_buf.add(sample_idx) };

                match bps {
                    8 => {
                        state.hasher.update((sample as i8).to_le_bytes());
                    }
                    16 => {
                        state.hasher.update((sample as i16).to_le_bytes());
                    }
                    24 => {
                        let bytes = sample.to_le_bytes();
                        state.hasher.update(&bytes[..3]);
                    }
                    32 => {
                        state.hasher.update(sample.to_le_bytes());
                    }
                    _ => {
                        // For non-standard bit depths, use the appropriate byte width
                        let byte_width = bps.div_ceil(8) as usize;
                        let bytes = sample.to_le_bytes();
                        state.hasher.update(&bytes[..byte_width]);
                    }
                }
            }
        }

        FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
    }

    unsafe extern "C" fn metadata_callback(
        _decoder: *const FLAC__StreamDecoder,
        metadata: *const FLAC__StreamMetadata,
        client_data: *mut c_void,
    ) {
        let state = unsafe { &mut *(client_data as *mut HasherState) };
        let metadata = unsafe { &*metadata };

        if metadata.type_ == FLAC__METADATA_TYPE_STREAMINFO {
            let si = unsafe { &metadata.data.stream_info };
            state.bits_per_sample = si.bits_per_sample;
        }
    }

    unsafe extern "C" fn error_callback(
        _decoder: *const FLAC__StreamDecoder,
        status: FLAC__StreamDecoderErrorStatus,
        client_data: *mut c_void,
    ) {
        let state = unsafe { &mut *(client_data as *mut HasherState) };
        state.error = Some(format!(
            "FLAC decode error during hashing: status {}",
            status
        ));
    }

    let mut state = HasherState {
        hasher: Md5::new(),
        bits_per_sample: 16, // default, will be overwritten by metadata callback
        error: None,
    };

    let path_cstr =
        CString::new(file_path.to_string_lossy().as_bytes()).map_err(|_| "Invalid file path")?;

    unsafe {
        let decoder = FLAC__stream_decoder_new();
        if decoder.is_null() {
            return Err("Failed to create FLAC decoder for hashing".to_string());
        }

        let init_status = FLAC__stream_decoder_init_file(
            decoder,
            path_cstr.as_ptr(),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            &mut state as *mut _ as *mut c_void,
        );

        if init_status != FLAC__STREAM_DECODER_INIT_STATUS_OK {
            FLAC__stream_decoder_delete(decoder);
            return Err(format!(
                "Failed to init FLAC decoder for hashing: status {}",
                init_status
            ));
        }

        let ok = FLAC__stream_decoder_process_until_end_of_stream(decoder);
        FLAC__stream_decoder_finish(decoder);
        FLAC__stream_decoder_delete(decoder);

        if ok == 0 {
            return Err(state
                .error
                .unwrap_or_else(|| "FLAC decoding failed during hashing".to_string()));
        }
        if let Some(err) = state.error {
            return Err(err);
        }
    }

    let result = state.hasher.finalize();
    Ok(format!("{:032x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash of zero bytes (empty input) in MD5.
    const MD5_EMPTY: &str = "d41d8cd98f00b204e9800998ecf8427e";

    #[test]
    fn test_hash_decoded_audio_returns_32_hex_chars_on_real_flac() {
        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");

        if !flac_path.exists() {
            eprintln!("Test FLAC not found at {:?} — skipping", flac_path);
            return;
        }

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(hash_decoded_audio(&flac_path));

        let hash = result.expect("hash_decoded_audio should succeed on test FLAC");
        assert_eq!(
            hash.len(),
            32,
            "MD5 hex string must be 32 characters, got: {hash}"
        );
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "MD5 must be lowercase hex digits, got: {hash}"
        );
        // The actual audio is not silence, so hash must not be the MD5 of empty bytes
        assert_ne!(
            hash, MD5_EMPTY,
            "real audio hash must not be MD5 of empty bytes"
        );
    }

    #[test]
    fn test_hash_decoded_audio_is_deterministic() {
        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");

        if !flac_path.exists() {
            eprintln!("Test FLAC not found — skipping determinism test");
            return;
        }

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let h1 = rt
            .block_on(hash_decoded_audio(&flac_path))
            .expect("first hash");
        let h2 = rt
            .block_on(hash_decoded_audio(&flac_path))
            .expect("second hash");
        assert_eq!(h1, h2, "hash must be deterministic");
    }

    #[test]
    fn test_hash_decoded_audio_errors_on_missing_file() {
        let missing = Path::new("/nonexistent/path/that/does/not/exist.flac");
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(hash_decoded_audio(missing));
        assert!(result.is_err(), "should error on missing file");
    }
}
