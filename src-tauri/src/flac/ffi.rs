//! Small helpers shared by the libFLAC wrappers.

use libflac_sys::*;
use std::ffi::{CStr, CString};
use std::path::Path;

/// Convert a path to a NUL-terminated string for libFLAC. libFLAC is built
/// with UTF-8 filename support on Windows, so lossy UTF-8 is correct here.
pub fn path_cstring(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| format!("Path contains a NUL byte: {}", path.display()))
}

/// Read entry `index` of one of libFLAC's exported `const char *[]` tables.
///
/// # Safety
/// `table` must be the address of a libFLAC string table and `index` a valid
/// state/status value for that table.
unsafe fn table_entry(table: *const *const libc::c_char, index: u32) -> String {
    let entry = *table.add(index as usize);
    if entry.is_null() {
        format!("#{index}")
    } else {
        CStr::from_ptr(entry).to_string_lossy().into_owned()
    }
}

pub fn decoder_state_string(state: FLAC__StreamDecoderState) -> String {
    // The decoder has 10 states; anything else is garbage from a bad cast.
    if state > FLAC__STREAM_DECODER_UNINITIALIZED {
        return format!("#{state}");
    }
    unsafe { table_entry(FLAC__StreamDecoderStateString.as_ptr(), state) }
}

pub fn decoder_init_status_string(status: FLAC__StreamDecoderInitStatus) -> String {
    if status > FLAC__STREAM_DECODER_INIT_STATUS_ALREADY_INITIALIZED {
        return format!("#{status}");
    }
    unsafe { table_entry(FLAC__StreamDecoderInitStatusString.as_ptr(), status) }
}

pub fn decoder_error_status_string(status: FLAC__StreamDecoderErrorStatus) -> String {
    if status > FLAC__STREAM_DECODER_ERROR_STATUS_BAD_METADATA {
        return format!("#{status}");
    }
    unsafe { table_entry(FLAC__StreamDecoderErrorStatusString.as_ptr(), status) }
}

pub fn encoder_state_string(state: FLAC__StreamEncoderState) -> String {
    if state > FLAC__STREAM_ENCODER_FRAMING_ERROR {
        return format!("#{state}");
    }
    unsafe { table_entry(FLAC__StreamEncoderStateString.as_ptr(), state) }
}

pub fn encoder_init_status_string(status: FLAC__StreamEncoderInitStatus) -> String {
    if status > FLAC__STREAM_ENCODER_INIT_STATUS_ALREADY_INITIALIZED {
        return format!("#{status}");
    }
    unsafe { table_entry(FLAC__StreamEncoderInitStatusString.as_ptr(), status) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_strings_are_human_readable() {
        assert_eq!(
            decoder_state_string(FLAC__STREAM_DECODER_END_OF_STREAM),
            "FLAC__STREAM_DECODER_END_OF_STREAM"
        );
        assert_eq!(
            encoder_state_string(FLAC__STREAM_ENCODER_VERIFY_MISMATCH_IN_AUDIO_DATA),
            "FLAC__STREAM_ENCODER_VERIFY_MISMATCH_IN_AUDIO_DATA"
        );
        assert_eq!(
            encoder_init_status_string(FLAC__STREAM_ENCODER_INIT_STATUS_OK),
            "FLAC__STREAM_ENCODER_INIT_STATUS_OK"
        );
    }

    #[test]
    fn test_out_of_range_states_do_not_crash() {
        assert_eq!(decoder_state_string(9999), "#9999");
        assert_eq!(encoder_state_string(9999), "#9999");
    }

    #[test]
    fn test_path_cstring_rejects_nul() {
        assert!(path_cstring(Path::new("a\0b")).is_err());
        assert!(path_cstring(Path::new("/tmp/ok.flac")).is_ok());
    }
}
