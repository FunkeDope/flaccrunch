use crate::flac::metadata::{self, PictureBlock};
use std::path::Path;

/// Extract all PICTURE blocks info from a FLAC file.
pub async fn extract_picture_blocks(
    flac_path: &Path,
) -> Result<Vec<PictureBlock>, String> {
    metadata::list_picture_blocks(flac_path).await
}

/// Export a specific picture block's image data to a file.
pub async fn export_picture_to_file(
    flac_path: &Path,
    block: &PictureBlock,
    output_path: &Path,
) -> Result<(), String> {
    metadata::export_picture(flac_path, block.block_number, output_path).await
}
