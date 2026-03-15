use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Result of a FLAC encoding operation.
#[derive(Debug)]
pub struct EncodeResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

/// Encode a FLAC file at maximum compression level.
/// Uses: flac -8 -e -p -V -f -o <output> -- <input>
pub async fn encode_flac(
    flac_bin: &Path,
    input: &Path,
    output: &Path,
) -> Result<EncodeResult, String> {
    let mut cmd = Command::new(flac_bin);
    cmd.args([
        "-8",    // Maximum compression level
        "-e",    // Exhaustive model search
        "-p",    // Do exhaustive search of LP coefficient quantization
        "-V",    // Verify the encoding
        "-f",    // Force overwrite of output file
        "-o",
    ]);
    cmd.arg(output);
    cmd.arg("--");
    cmd.arg(input);

    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn flac: {e}"))?;
    let output_result = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait for flac: {e}"))?;

    let stderr = String::from_utf8_lossy(&output_result.stderr).to_string();
    let exit_code = output_result.status.code();
    let success = output_result.status.success();

    Ok(EncodeResult {
        success,
        exit_code,
        stderr,
    })
}

/// Build the flac encoding arguments as a Vec<String> (for display/logging).
pub fn build_flac_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-8".to_string(),
        "-e".to_string(),
        "-p".to_string(),
        "-V".to_string(),
        "-f".to_string(),
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
        assert_eq!(args.len(), 9);
        assert_eq!(args[0], "-8");
        assert_eq!(args[5], "-o");
        assert_eq!(args[6], "/music/track.tmp");
        assert_eq!(args[7], "--");
        assert_eq!(args[8], "/music/track.flac");
    }

    #[test]
    fn test_build_flac_args_spaces_in_path() {
        let args = build_flac_args(
            &PathBuf::from("/my music/a track.flac"),
            &PathBuf::from("/my music/a track.tmp"),
        );
        assert!(args[6].contains("my music"));
        assert!(args[8].contains("a track"));
    }

    #[test]
    fn test_build_flac_args_unicode_path() {
        let args = build_flac_args(
            &PathBuf::from("/音楽/トラック.flac"),
            &PathBuf::from("/音楽/トラック.tmp"),
        );
        assert!(args[8].contains("トラック"));
    }
}
