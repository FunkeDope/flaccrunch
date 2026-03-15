/// Result of optimizing a JPEG image.
#[derive(Debug)]
pub struct JpegOptimizeResult {
    pub original_size: u64,
    pub optimized_size: u64,
    pub saved_bytes: i64,
}

/// Optimize JPEG data losslessly.
/// Uses jpegtran-style optimization: Huffman table optimization, preserving all metadata.
///
/// For now, this uses a simple approach since mozjpeg requires C compilation.
/// In production, integrate the `mozjpeg` or `turbojpeg` crate for proper optimization.
pub fn optimize_jpeg(input_data: &[u8]) -> Result<Option<Vec<u8>>, String> {
    // Validate JPEG header
    if input_data.len() < 3 || input_data[..3] != [0xFF, 0xD8, 0xFF] {
        return Err("Invalid JPEG data".to_string());
    }

    // For initial implementation, we'll use a subprocess call to jpegtran if available,
    // or return None (no optimization) if not.
    // The mozjpeg crate integration will be added as an enhancement.
    //
    // This is a placeholder that returns None to indicate no optimization was performed.
    // When mozjpeg is integrated, this will return Some(optimized_bytes) when the
    // optimized version is smaller.
    Ok(None)
}

/// Optimize a JPEG file in-place using an external jpegtran binary if available.
pub async fn optimize_jpeg_file(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<JpegOptimizeResult, String> {
    let original_data =
        std::fs::read(input_path).map_err(|e| format!("Failed to read JPEG: {e}"))?;
    let original_size = original_data.len() as u64;

    // Try to find jpegtran in PATH
    if let Ok(jpegtran) = which::which("jpegtran") {
        let output = tokio::process::Command::new(jpegtran)
            .args(["-copy", "all", "-optimize", "-outfile"])
            .arg(output_path)
            .arg(input_path)
            .output()
            .await
            .map_err(|e| format!("Failed to run jpegtran: {e}"))?;

        if output.status.success() {
            let optimized_size = std::fs::metadata(output_path)
                .map_err(|e| format!("Failed to read optimized JPEG: {e}"))?
                .len();

            return Ok(JpegOptimizeResult {
                original_size,
                optimized_size,
                saved_bytes: original_size as i64 - optimized_size as i64,
            });
        }
    }

    // No optimization performed
    Ok(JpegOptimizeResult {
        original_size,
        optimized_size: original_size,
        saved_bytes: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimize_jpeg_invalid_data() {
        let result = optimize_jpeg(&[0x00, 0x01, 0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn test_optimize_jpeg_valid_header() {
        // Minimal valid JPEG header
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = optimize_jpeg(&data);
        assert!(result.is_ok());
    }
}
