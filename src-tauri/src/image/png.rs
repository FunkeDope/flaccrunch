use std::fs;
use std::path::Path;

/// Result of optimizing a PNG image.
#[derive(Debug)]
pub struct PngOptimizeResult {
    pub original_size: u64,
    pub optimized_size: u64,
    pub saved_bytes: i64,
}

/// Optimize a PNG file in-place using the oxipng library.
/// Uses optimization level 4 (equivalent to `oxipng -o 4`).
pub fn optimize_png(path: &Path) -> Result<PngOptimizeResult, String> {
    let original_data = fs::read(path).map_err(|e| format!("Failed to read PNG: {e}"))?;
    let original_size = original_data.len() as u64;

    let options = oxipng::Options {
        optimize_alpha: true,
        ..oxipng::Options::from_preset(4)
    };

    oxipng::optimize(
        &oxipng::InFile::Path(path.to_path_buf()),
        &oxipng::OutFile::Path {
            path: Some(path.to_path_buf()),
            preserve_attrs: true,
        },
        &options,
    )
    .map_err(|e| format!("oxipng optimization failed: {e}"))?;

    let optimized_size = fs::metadata(path)
        .map_err(|e| format!("Failed to read optimized PNG metadata: {e}"))?
        .len();

    Ok(PngOptimizeResult {
        original_size,
        optimized_size,
        saved_bytes: original_size as i64 - optimized_size as i64,
    })
}
