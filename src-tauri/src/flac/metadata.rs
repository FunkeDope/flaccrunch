use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

/// Represents a PICTURE block in a FLAC file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PictureBlock {
    pub block_number: u32,
    pub picture_type: u32,
    pub mime_type: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub colors: u32,
    pub data_length: u64,
}

/// Get the embedded MD5 hash from a FLAC file's STREAMINFO.
pub async fn get_md5sum(
    metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<Option<String>, String> {
    let output = Command::new(metaflac_bin)
        .args(["--show-md5sum", "--"])
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run metaflac: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "metaflac --show-md5sum failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let md5 = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if md5.is_empty() {
        Ok(None)
    } else {
        Ok(Some(md5))
    }
}

/// List all PICTURE blocks in a FLAC file.
pub async fn list_picture_blocks(
    metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<Vec<PictureBlock>, String> {
    let output = Command::new(metaflac_bin)
        .args(["--list", "--block-type=PICTURE", "--"])
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run metaflac: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // No PICTURE blocks is not an error
        if stderr.contains("no PICTURE block") || output.stdout.is_empty() {
            return Ok(Vec::new());
        }
        return Err(format!("metaflac --list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_picture_block_listing(&stdout))
}

/// Parse the output of `metaflac --list --block-type=PICTURE`.
pub fn parse_picture_block_listing(output: &str) -> Vec<PictureBlock> {
    let mut blocks = Vec::new();
    let mut current_block_num: Option<u32> = None;
    let mut picture_type: u32 = 0;
    let mut mime_type = String::new();
    let mut description = String::new();
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut depth: u32 = 0;
    let mut colors: u32 = 0;
    let mut data_length: u64 = 0;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("METADATA block #") {
            // Save previous block if any
            if let Some(bn) = current_block_num {
                blocks.push(PictureBlock {
                    block_number: bn,
                    picture_type,
                    mime_type: mime_type.clone(),
                    description: description.clone(),
                    width,
                    height,
                    depth,
                    colors,
                    data_length,
                });
            }
            // Parse new block number
            current_block_num = trimmed
                .strip_prefix("METADATA block #")
                .and_then(|s| s.parse::<u32>().ok());
            // Reset fields
            picture_type = 0;
            mime_type.clear();
            description.clear();
            width = 0;
            height = 0;
            depth = 0;
            colors = 0;
            data_length = 0;
        } else if let Some(val) = trimmed.strip_prefix("type: ") {
            // Type line format: "type: 3 (Cover (front))"
            picture_type = val
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if let Some(val) = trimmed.strip_prefix("MIME type: ") {
            mime_type = val.to_string();
        } else if let Some(val) = trimmed.strip_prefix("description: ") {
            description = val.to_string();
        } else if let Some(val) = trimmed.strip_prefix("width: ") {
            width = val.parse().unwrap_or(0);
        } else if let Some(val) = trimmed.strip_prefix("height: ") {
            height = val.parse().unwrap_or(0);
        } else if let Some(val) = trimmed.strip_prefix("depth: ") {
            depth = val.parse().unwrap_or(0);
        } else if let Some(val) = trimmed.strip_prefix("colors: ") {
            colors = val.parse().unwrap_or(0);
        } else if let Some(val) = trimmed.strip_prefix("data length: ") {
            data_length = val.parse().unwrap_or(0);
        }
    }

    // Save last block
    if let Some(bn) = current_block_num {
        blocks.push(PictureBlock {
            block_number: bn,
            picture_type,
            mime_type,
            description,
            width,
            height,
            depth,
            colors,
            data_length,
        });
    }

    blocks
}

/// Export a PICTURE block from a FLAC file to a file.
pub async fn export_picture(
    metaflac_bin: &Path,
    flac_path: &Path,
    block_number: u32,
    output_path: &Path,
) -> Result<(), String> {
    let output = Command::new(metaflac_bin)
        .args([
            &format!("--block-number={block_number}"),
            "--export-picture-to",
        ])
        .arg(output_path)
        .arg("--")
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to export picture: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "metaflac --export-picture-to failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Remove a specific metadata block from a FLAC file.
pub async fn remove_block(
    metaflac_bin: &Path,
    flac_path: &Path,
    block_number: u32,
) -> Result<(), String> {
    let output = Command::new(metaflac_bin)
        .args([
            &format!("--block-number={block_number}"),
            "--remove",
            "--dont-use-padding",
            "--",
        ])
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to remove block: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "metaflac --remove failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Import a picture into a FLAC file using a spec string.
/// Spec format: "TYPE|MIME|DESCRIPTION|WIDTHxHEIGHTxDEPTH/COLORS|FILE"
pub async fn import_picture(
    metaflac_bin: &Path,
    flac_path: &Path,
    spec: &str,
) -> Result<(), String> {
    let output = Command::new(metaflac_bin)
        .args(["--dont-use-padding", &format!("--import-picture-from={spec}"), "--"])
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to import picture: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "metaflac --import-picture-from failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Remove all PADDING blocks from a FLAC file.
pub async fn remove_padding(
    metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<(), String> {
    let output = Command::new(metaflac_bin)
        .args([
            "--remove",
            "--block-type=PADDING",
            "--dont-use-padding",
            "--",
        ])
        .arg(flac_path)
        .output()
        .await
        .map_err(|e| format!("Failed to remove padding: {e}"))?;

    // Padding removal can "fail" if there's no padding — that's fine
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("no PADDING block") {
            return Err(format!("metaflac remove padding failed: {stderr}"));
        }
    }
    Ok(())
}

/// Build a picture import spec string for metaflac.
/// Returns None if the MIME type is a URL reference ("-->") or description has newlines.
pub fn build_picture_spec(block: &PictureBlock, image_path: &Path) -> Option<String> {
    // Skip URL-type picture references
    if block.mime_type == "-->" {
        return None;
    }
    // Skip descriptions with newlines (metaflac can't handle them in spec)
    if block.description.contains('\n') {
        return None;
    }

    Some(format!(
        "{}|{}|{}|{}x{}x{}/{}|{}",
        block.picture_type,
        block.mime_type,
        block.description,
        block.width,
        block.height,
        block.depth,
        block.colors,
        image_path.to_string_lossy()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SAMPLE_LISTING: &str = r#"METADATA block #2
  type: 6 (PICTURE)
  MIME type: image/png
  description:
  width: 500
  height: 500
  depth: 24
  colors: 0
  data length: 123456
METADATA block #3
  type: 6 (PICTURE)
  MIME type: image/jpeg
  description: Back cover
  width: 600
  height: 600
  depth: 24
  colors: 0
  data length: 78901
"#;

    #[test]
    fn test_parse_picture_block_listing_multiple() {
        let blocks = parse_picture_block_listing(SAMPLE_LISTING);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_number, 2);
        assert_eq!(blocks[0].mime_type, "image/png");
        assert_eq!(blocks[0].width, 500);
        assert_eq!(blocks[0].data_length, 123456);
        assert_eq!(blocks[1].block_number, 3);
        assert_eq!(blocks[1].mime_type, "image/jpeg");
        assert_eq!(blocks[1].description, "Back cover");
    }

    #[test]
    fn test_parse_picture_block_listing_single() {
        let input = r#"METADATA block #1
  type: 3 (Cover (front))
  MIME type: image/png
  description: Cover
  width: 1000
  height: 1000
  depth: 32
  colors: 0
  data length: 500000
"#;
        let blocks = parse_picture_block_listing(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].picture_type, 3);
        assert_eq!(blocks[0].description, "Cover");
    }

    #[test]
    fn test_parse_picture_block_listing_empty() {
        let blocks = parse_picture_block_listing("");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_build_picture_spec_front_cover() {
        let block = PictureBlock {
            block_number: 2,
            picture_type: 3,
            mime_type: "image/png".to_string(),
            description: "Cover".to_string(),
            width: 500,
            height: 500,
            depth: 24,
            colors: 0,
            data_length: 0,
        };
        let spec = build_picture_spec(&block, &PathBuf::from("/tmp/cover.png"));
        assert_eq!(spec.unwrap(), "3|image/png|Cover|500x500x24/0|/tmp/cover.png");
    }

    #[test]
    fn test_build_picture_spec_url_mime_returns_none() {
        let block = PictureBlock {
            block_number: 2,
            picture_type: 3,
            mime_type: "-->".to_string(),
            description: String::new(),
            width: 0,
            height: 0,
            depth: 0,
            colors: 0,
            data_length: 0,
        };
        assert!(build_picture_spec(&block, &PathBuf::from("/tmp/img.png")).is_none());
    }

    #[test]
    fn test_build_picture_spec_newline_description_returns_none() {
        let block = PictureBlock {
            block_number: 2,
            picture_type: 3,
            mime_type: "image/png".to_string(),
            description: "Line1\nLine2".to_string(),
            width: 500,
            height: 500,
            depth: 24,
            colors: 0,
            data_length: 0,
        };
        assert!(build_picture_spec(&block, &PathBuf::from("/tmp/img.png")).is_none());
    }
}
