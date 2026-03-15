use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::path::Path;

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

/// Get the embedded MD5 hash from a FLAC file's STREAMINFO using native libFLAC.
pub async fn get_md5sum(
    _metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<Option<String>, String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || get_md5sum_native(&flac_path))
        .await
        .map_err(|e| format!("MD5 task panicked: {e}"))?
}

fn get_md5sum_native(flac_path: &Path) -> Result<Option<String>, String> {
    use libflac_sys::*;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;

    unsafe {
        // IMPORTANT: Must allocate a full FLAC__StreamMetadata, not just StreamInfo.
        // FLAC__metadata_get_streaminfo writes the full metadata struct including
        // type, is_last, length fields before the stream_info union data.
        let mut metadata: FLAC__StreamMetadata = std::mem::zeroed();
        let ok = FLAC__metadata_get_streaminfo(path_cstr.as_ptr(), &mut metadata);

        if ok == 0 {
            return Err("Failed to read STREAMINFO".to_string());
        }

        let md5_bytes = metadata.data.stream_info.md5sum;
        let md5_str: String = md5_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        Ok(Some(md5_str))
    }
}

/// List all PICTURE blocks in a FLAC file using native libFLAC metadata API.
pub async fn list_picture_blocks(
    _metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<Vec<PictureBlock>, String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || list_picture_blocks_native(&flac_path))
        .await
        .map_err(|e| format!("List pictures task panicked: {e}"))?
}

fn list_picture_blocks_native(flac_path: &Path) -> Result<Vec<PictureBlock>, String> {
    use libflac_sys::*;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;

    let mut blocks = Vec::new();

    unsafe {
        let chain = FLAC__metadata_chain_new();
        if chain.is_null() {
            return Err("Failed to create metadata chain".to_string());
        }

        if FLAC__metadata_chain_read(chain, path_cstr.as_ptr()) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to read metadata chain".to_string());
        }

        let iter = FLAC__metadata_iterator_new();
        if iter.is_null() {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create metadata iterator".to_string());
        }

        FLAC__metadata_iterator_init(iter, chain);

        let mut block_number: u32 = 0;
        loop {
            let block = FLAC__metadata_iterator_get_block(iter);
            if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_PICTURE {
                let pic = &(*block).data.picture;
                let mime = if pic.mime_type.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(pic.mime_type).to_string_lossy().into_owned()
                };
                let desc = if pic.description.is_null() {
                    String::new()
                } else {
                    // description is FLAC__byte* (u8*), treat as UTF-8
                    let desc_cstr = std::ffi::CStr::from_ptr(pic.description as *const std::os::raw::c_char);
                    desc_cstr.to_string_lossy().into_owned()
                };

                blocks.push(PictureBlock {
                    block_number,
                    picture_type: pic.type_ as u32,
                    mime_type: mime,
                    description: desc,
                    width: pic.width,
                    height: pic.height,
                    depth: pic.depth,
                    colors: pic.colors,
                    data_length: pic.data_length as u64,
                });
            }
            block_number += 1;

            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        FLAC__metadata_iterator_delete(iter);
        FLAC__metadata_chain_delete(chain);
    }

    Ok(blocks)
}

/// Export a PICTURE block from a FLAC file to a file using native libFLAC.
pub async fn export_picture(
    _metaflac_bin: &Path,
    flac_path: &Path,
    block_number: u32,
    output_path: &Path,
) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    let output_path = output_path.to_path_buf();
    tokio::task::spawn_blocking(move || export_picture_native(&flac_path, block_number, &output_path))
        .await
        .map_err(|e| format!("Export picture task panicked: {e}"))?
}

fn export_picture_native(flac_path: &Path, target_block: u32, output_path: &Path) -> Result<(), String> {
    use libflac_sys::*;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;

    unsafe {
        let chain = FLAC__metadata_chain_new();
        if chain.is_null() {
            return Err("Failed to create metadata chain".to_string());
        }

        if FLAC__metadata_chain_read(chain, path_cstr.as_ptr()) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to read metadata chain".to_string());
        }

        let iter = FLAC__metadata_iterator_new();
        if iter.is_null() {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create metadata iterator".to_string());
        }

        FLAC__metadata_iterator_init(iter, chain);

        let mut block_idx: u32 = 0;
        let mut found = false;
        loop {
            let block = FLAC__metadata_iterator_get_block(iter);
            if block_idx == target_block && !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_PICTURE {
                let pic = &(*block).data.picture;
                if !pic.data.is_null() && pic.data_length > 0 {
                    let data = std::slice::from_raw_parts(pic.data, pic.data_length as usize);
                    std::fs::write(&output_path, data)
                        .map_err(|e| format!("Failed to write picture: {e}"))?;
                    found = true;
                }
                break;
            }
            block_idx += 1;
            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        FLAC__metadata_iterator_delete(iter);
        FLAC__metadata_chain_delete(chain);

        if !found {
            return Err(format!("Picture block #{} not found", target_block));
        }
    }

    Ok(())
}

/// Remove a specific metadata block from a FLAC file using native libFLAC.
pub async fn remove_block(
    _metaflac_bin: &Path,
    flac_path: &Path,
    block_number: u32,
) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || remove_block_native(&flac_path, block_number))
        .await
        .map_err(|e| format!("Remove block task panicked: {e}"))?
}

fn remove_block_native(flac_path: &Path, target_block: u32) -> Result<(), String> {
    use libflac_sys::*;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;

    unsafe {
        let chain = FLAC__metadata_chain_new();
        if chain.is_null() {
            return Err("Failed to create metadata chain".to_string());
        }

        if FLAC__metadata_chain_read(chain, path_cstr.as_ptr()) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to read metadata chain".to_string());
        }

        let iter = FLAC__metadata_iterator_new();
        if iter.is_null() {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create metadata iterator".to_string());
        }

        FLAC__metadata_iterator_init(iter, chain);

        let mut block_idx: u32 = 0;
        loop {
            if block_idx == target_block {
                FLAC__metadata_iterator_delete_block(iter, 0); // 0 = don't replace with padding
                break;
            }
            block_idx += 1;
            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        FLAC__metadata_iterator_delete(iter);

        // Write changes back
        FLAC__metadata_chain_sort_padding(chain);
        if FLAC__metadata_chain_write(chain, 1, 0) == 0 { // use_padding=1, preserve_file_stats=0
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to write metadata chain".to_string());
        }

        FLAC__metadata_chain_delete(chain);
    }

    Ok(())
}

/// Import a picture into a FLAC file using a spec string.
/// Spec format: "TYPE|MIME|DESCRIPTION|WIDTHxHEIGHTxDEPTH/COLORS|FILE"
pub async fn import_picture(
    _metaflac_bin: &Path,
    flac_path: &Path,
    spec: &str,
) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    let spec = spec.to_string();
    tokio::task::spawn_blocking(move || import_picture_native(&flac_path, &spec))
        .await
        .map_err(|e| format!("Import picture task panicked: {e}"))?
}

fn import_picture_native(flac_path: &Path, spec: &str) -> Result<(), String> {
    use libflac_sys::*;

    // Parse spec: "TYPE|MIME|DESCRIPTION|WIDTHxHEIGHTxDEPTH/COLORS|FILE"
    let parts: Vec<&str> = spec.splitn(5, '|').collect();
    if parts.len() != 5 {
        return Err(format!("Invalid picture spec: {}", spec));
    }

    let pic_type: u32 = parts[0].parse().unwrap_or(3);
    let mime = parts[1];
    let description = parts[2];
    let image_file = parts[4];

    // Read the image file
    let image_data = std::fs::read(image_file)
        .map_err(|e| format!("Failed to read image file '{}': {}", image_file, e))?;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;
    let mime_cstr = CString::new(mime).map_err(|_| "Invalid MIME type")?;
    let desc_cstr = CString::new(description).map_err(|_| "Invalid description")?;

    // Parse dimensions from spec part 3: "WIDTHxHEIGHTxDEPTH/COLORS"
    let (width, height, depth, colors) = parse_dimensions(parts[3]);

    unsafe {
        let chain = FLAC__metadata_chain_new();
        if chain.is_null() {
            return Err("Failed to create metadata chain".to_string());
        }

        if FLAC__metadata_chain_read(chain, path_cstr.as_ptr()) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to read metadata chain".to_string());
        }

        let iter = FLAC__metadata_iterator_new();
        if iter.is_null() {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create metadata iterator".to_string());
        }

        FLAC__metadata_iterator_init(iter, chain);

        // Move to end
        while FLAC__metadata_iterator_next(iter) != 0 {}

        // Create new PICTURE block
        let picture = FLAC__metadata_object_new(FLAC__METADATA_TYPE_PICTURE);
        if picture.is_null() {
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create picture metadata object".to_string());
        }

        (*picture).data.picture.type_ = pic_type as FLAC__StreamMetadata_Picture_Type;

        FLAC__metadata_object_picture_set_mime_type(
            picture,
            mime_cstr.as_ptr() as *mut std::os::raw::c_char,
            1, // copy
        );
        FLAC__metadata_object_picture_set_description(
            picture,
            desc_cstr.as_ptr() as *mut u8,
            1, // copy
        );
        FLAC__metadata_object_picture_set_data(
            picture,
            image_data.as_ptr() as *mut u8,
            image_data.len() as u32,
            1, // copy
        );

        (*picture).data.picture.width = width;
        (*picture).data.picture.height = height;
        (*picture).data.picture.depth = depth;
        (*picture).data.picture.colors = colors;

        if FLAC__metadata_iterator_insert_block_after(iter, picture) == 0 {
            FLAC__metadata_object_delete(picture);
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to insert picture block".to_string());
        }

        FLAC__metadata_iterator_delete(iter);

        if FLAC__metadata_chain_write(chain, 1, 0) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to write metadata chain after picture import".to_string());
        }

        FLAC__metadata_chain_delete(chain);
    }

    Ok(())
}

fn parse_dimensions(dim_str: &str) -> (u32, u32, u32, u32) {
    // Format: "WIDTHxHEIGHTxDEPTH/COLORS"
    let (dims, colors_str) = dim_str.split_once('/').unwrap_or((dim_str, "0"));
    let colors: u32 = colors_str.parse().unwrap_or(0);

    let parts: Vec<&str> = dims.split('x').collect();
    let width: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let height: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let depth: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    (width, height, depth, colors)
}

/// Remove all PADDING blocks from a FLAC file using native libFLAC.
pub async fn remove_padding(
    _metaflac_bin: &Path,
    flac_path: &Path,
) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || remove_padding_native(&flac_path))
        .await
        .map_err(|e| format!("Remove padding task panicked: {e}"))?
}

fn remove_padding_native(flac_path: &Path) -> Result<(), String> {
    use libflac_sys::*;

    let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid path")?;

    unsafe {
        let chain = FLAC__metadata_chain_new();
        if chain.is_null() {
            return Err("Failed to create metadata chain".to_string());
        }

        if FLAC__metadata_chain_read(chain, path_cstr.as_ptr()) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to read metadata chain".to_string());
        }

        let iter = FLAC__metadata_iterator_new();
        if iter.is_null() {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to create metadata iterator".to_string());
        }

        FLAC__metadata_iterator_init(iter, chain);

        let mut removed_any = false;
        loop {
            let block = FLAC__metadata_iterator_get_block(iter);
            if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_PADDING {
                FLAC__metadata_iterator_delete_block(iter, 0);
                removed_any = true;
                // After deletion, iterator advances to next block, so don't call next
                continue;
            }
            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        FLAC__metadata_iterator_delete(iter);

        if removed_any {
            if FLAC__metadata_chain_write(chain, 0, 0) == 0 { // no padding
                FLAC__metadata_chain_delete(chain);
                return Err("Failed to write metadata chain after padding removal".to_string());
            }
        }

        FLAC__metadata_chain_delete(chain);
    }

    Ok(())
}

/// Build a picture import spec string for metaflac.
/// Returns None if the MIME type is a URL reference ("-->") or description has newlines.
pub fn build_picture_spec(block: &PictureBlock, image_path: &Path) -> Option<String> {
    if block.mime_type == "-->" {
        return None;
    }
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

/// Parse the output of `metaflac --list --block-type=PICTURE` (kept for compatibility).
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
            current_block_num = trimmed
                .strip_prefix("METADATA block #")
                .and_then(|s| s.parse::<u32>().ok());
            picture_type = 0;
            mime_type.clear();
            description.clear();
            width = 0;
            height = 0;
            depth = 0;
            colors = 0;
            data_length = 0;
        } else if let Some(val) = trimmed.strip_prefix("type: ") {
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
    fn test_parse_dimensions() {
        assert_eq!(parse_dimensions("500x500x24/0"), (500, 500, 24, 0));
        assert_eq!(parse_dimensions("1920x1080x32/256"), (1920, 1080, 32, 256));
    }
}
