//! MD5 of decoded audio, computed the same way libFLAC defines the STREAMINFO
//! MD5: interleaved little-endian signed PCM, `ceil(bps/8)` bytes per sample.

use crate::flac::ffi::{
    decoder_error_status_string, decoder_init_status_string, decoder_state_string, path_cstring,
};
use libflac_sys::*;
use md5::{Digest, Md5};
use std::os::raw::c_void;
use std::path::Path;

/// Compute the MD5 hash of the decoded audio stream from a FLAC file.
pub async fn hash_decoded_audio(file_path: &Path) -> Result<String, String> {
    let file_path = file_path.to_path_buf();
    tokio::task::spawn_blocking(move || hash_decoded_audio_native(&file_path))
        .await
        .map_err(|e| format!("Hash task panicked: {e}"))?
}

struct HasherState {
    hasher: Md5,
    bytes_per_sample: usize,
    /// Reused across frames to avoid per-frame allocation.
    buf: Vec<u8>,
    error: Option<String>,
}

unsafe extern "C" fn write_callback(
    _decoder: *const FLAC__StreamDecoder,
    frame: *const FLAC__Frame,
    buffer: *const *const FLAC__int32,
    client_data: *mut c_void,
) -> FLAC__StreamDecoderWriteStatus {
    let state = &mut *(client_data as *mut HasherState);
    let blocksize = (*frame).header.blocksize as usize;
    let channels = (*frame).header.channels as usize;
    let bps = state.bytes_per_sample;

    // Serialize the whole frame, then hash it in one call. Hashing sample by
    // sample (the previous approach) spent most of its time in MD5 call
    // overhead rather than in the compression function.
    state.buf.clear();
    state.buf.reserve(blocksize * channels * bps);
    for i in 0..blocksize {
        for ch in 0..channels {
            let sample = *(*buffer.add(ch)).add(i);
            let le = sample.to_le_bytes();
            state.buf.extend_from_slice(&le[..bps]);
        }
    }
    state.hasher.update(&state.buf);
    FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
}

unsafe extern "C" fn metadata_callback(
    _decoder: *const FLAC__StreamDecoder,
    metadata: *const FLAC__StreamMetadata,
    client_data: *mut c_void,
) {
    let state = &mut *(client_data as *mut HasherState);
    if (*metadata).type_ == FLAC__METADATA_TYPE_STREAMINFO {
        let bps = (*metadata).data.stream_info.bits_per_sample;
        state.bytes_per_sample = (bps.div_ceil(8) as usize).clamp(1, 4);
    }
}

unsafe extern "C" fn error_callback(
    _decoder: *const FLAC__StreamDecoder,
    status: FLAC__StreamDecoderErrorStatus,
    client_data: *mut c_void,
) {
    let state = &mut *(client_data as *mut HasherState);
    if state.error.is_none() {
        state.error = Some(format!(
            "FLAC decode error during hashing: {}",
            decoder_error_status_string(status)
        ));
    }
}

struct DecoderGuard(*mut FLAC__StreamDecoder);
impl Drop for DecoderGuard {
    fn drop(&mut self) {
        unsafe { FLAC__stream_decoder_delete(self.0) }
    }
}

fn hash_decoded_audio_native(file_path: &Path) -> Result<String, String> {
    let path_c = path_cstring(file_path)?;
    let mut state = HasherState {
        hasher: Md5::new(),
        bytes_per_sample: 2,
        buf: Vec::with_capacity(4096 * 2 * 2),
        error: None,
    };

    unsafe {
        let guard = DecoderGuard(FLAC__stream_decoder_new());
        let decoder = guard.0;
        if decoder.is_null() {
            return Err("Failed to create FLAC decoder for hashing".to_string());
        }

        let init = FLAC__stream_decoder_init_file(
            decoder,
            path_c.as_ptr(),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            &mut state as *mut _ as *mut c_void,
        );
        if init != FLAC__STREAM_DECODER_INIT_STATUS_OK {
            return Err(format!(
                "Failed to init FLAC decoder for hashing: {}",
                decoder_init_status_string(init)
            ));
        }

        let ok = FLAC__stream_decoder_process_until_end_of_stream(decoder);
        FLAC__stream_decoder_finish(decoder);

        if let Some(err) = state.error.take() {
            return Err(err);
        }
        if ok == 0 {
            return Err(format!(
                "FLAC decoding failed during hashing: {}",
                decoder_state_string(FLAC__stream_decoder_get_state(decoder))
            ));
        }
    }

    Ok(format!("{:032x}", state.hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash of zero bytes (empty input) in MD5.
    const MD5_EMPTY: &str = "d41d8cd98f00b204e9800998ecf8427e";

    fn fixture() -> Option<std::path::PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        p.exists().then_some(p)
    }

    #[test]
    fn test_hash_decoded_audio_matches_embedded_md5_on_fixture() {
        let Some(flac_path) = fixture() else {
            eprintln!("Test FLAC not found — skipping");
            return;
        };
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let hash = rt.block_on(hash_decoded_audio(&flac_path)).expect("hash");
        assert_eq!(hash.len(), 32);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(hash, MD5_EMPTY);

        // The fixture carries a valid STREAMINFO MD5; our definition of the
        // decoded-audio hash must agree with libFLAC's.
        let embedded = rt
            .block_on(crate::flac::metadata::get_md5sum(&flac_path))
            .unwrap();
        if embedded != crate::util::format::NULL_MD5 {
            assert_eq!(hash, embedded, "decoded MD5 must match STREAMINFO MD5");
        }
    }

    #[test]
    fn test_hash_decoded_audio_is_deterministic() {
        let Some(flac_path) = fixture() else {
            return;
        };
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let h1 = rt.block_on(hash_decoded_audio(&flac_path)).unwrap();
        let h2 = rt.block_on(hash_decoded_audio(&flac_path)).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_decoded_audio_errors_on_missing_file() {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(hash_decoded_audio(Path::new("/nonexistent/x.flac")));
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_decoded_audio_errors_on_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("junk.flac");
        std::fs::write(&p, b"not a flac").unwrap();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        assert!(rt.block_on(hash_decoded_audio(&p)).is_err());
    }
}
