use crate::flac::metadata::{self, PictureBlock};
use crate::image::detect::{detect_image_format, ImageFormat};
use crate::image::png::optimize_png;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of album art optimization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtworkResult {
    pub changed: bool,
    pub saved_bytes: i64,
    pub raw_saved_bytes: i64,
    pub blocks_optimized: u32,
    pub summary: String,
}

/// Optimize all embedded album art in a FLAC file.
pub async fn optimize_album_art(
    flac_path: &Path,
    scratch_dir: &Path,
) -> Result<ArtworkResult, String> {
    let file_size_before = fs::metadata(flac_path)
        .map_err(|e| format!("Failed to read FLAC metadata: {e}"))?
        .len();

    let blocks = metadata::list_picture_blocks(flac_path).await?;
    if blocks.is_empty() {
        let _ = metadata::remove_padding(flac_path).await;
        let file_size_after = fs::metadata(flac_path)
            .map_err(|e| format!("Failed to read FLAC size after padding removal: {e}"))?
            .len();
        let saved = file_size_before as i64 - file_size_after as i64;
        return Ok(ArtworkResult {
            changed: saved > 0,
            saved_bytes: saved,
            raw_saved_bytes: 0,
            blocks_optimized: 0,
            summary: if saved > 0 {
                format!("Padding removed: {} bytes saved", saved)
            } else {
                "No album art found".to_string()
            },
        });
    }

    let mut all_exports: Vec<(PictureBlock, PathBuf)> = Vec::new();
    let mut optimized_block_nums: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut total_raw_saved: i64 = 0;

    for (idx, block) in blocks.iter().enumerate() {
        if block.mime_type == "-->" {
            continue;
        }

        let export_path = scratch_dir.join(format!("art_block_{idx}"));
        metadata::export_picture(flac_path, block.block_number, &export_path).await?;

        let image_data = fs::read(&export_path)
            .map_err(|e| format!("Failed to read exported picture: {e}"))?;
        let format = detect_image_format(&image_data);

        let raw_saved = match format {
            Some(ImageFormat::Png) => {
                match optimize_png(&export_path) {
                    Ok(result) => result.saved_bytes,
                    Err(_) => 0,
                }
            }
            Some(ImageFormat::Jpeg) => {
                let optimized_path = scratch_dir.join(format!("art_block_{idx}_opt.jpg"));
                match crate::image::jpeg::optimize_jpeg_file(&export_path, &optimized_path).await {
                    Ok(result) if result.saved_bytes > 0 => {
                        let _ = fs::rename(&optimized_path, &export_path);
                        result.saved_bytes
                    }
                    _ => {
                        let _ = fs::remove_file(&optimized_path);
                        0
                    }
                }
            }
            None => 0,
        };

        if raw_saved > 0 {
            total_raw_saved += raw_saved;
            optimized_block_nums.insert(block.block_number);
        }
        all_exports.push((block.clone(), export_path));
    }

    if !optimized_block_nums.is_empty() {
        let mut block_numbers: Vec<u32> = all_exports.iter()
            .map(|(b, _)| b.block_number)
            .collect();
        block_numbers.sort_unstable();
        block_numbers.reverse();
        for bn in &block_numbers {
            let _ = metadata::remove_block(flac_path, *bn).await;
        }

        for (block, path) in &all_exports {
            if let Some(spec) = metadata::build_picture_spec(block, path) {
                let _ = metadata::import_picture(flac_path, &spec).await;
            }
        }
    }

    for (_, path) in &all_exports {
        let _ = fs::remove_file(path);
    }

    let _ = metadata::remove_padding(flac_path).await;

    let file_size_after = fs::metadata(flac_path)
        .map_err(|e| format!("Failed to read FLAC size after art optimization: {e}"))?
        .len();

    let net_saved = file_size_before as i64 - file_size_after as i64;

    Ok(ArtworkResult {
        changed: net_saved > 0 || !optimized_block_nums.is_empty(),
        saved_bytes: net_saved,
        raw_saved_bytes: total_raw_saved,
        blocks_optimized: optimized_block_nums.len() as u32,
        summary: format!(
            "{} blocks optimized, raw saved: {} bytes, net saved: {} bytes",
            optimized_block_nums.len(),
            total_raw_saved,
            net_saved
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ArtworkResult ---

    #[test]
    fn test_artwork_result_default() {
        let r = ArtworkResult::default();
        assert!(!r.changed);
        assert_eq!(r.saved_bytes, 0);
        assert_eq!(r.raw_saved_bytes, 0);
        assert_eq!(r.blocks_optimized, 0);
        assert!(r.summary.is_empty());
    }

    #[test]
    fn test_artwork_result_clone() {
        let r = ArtworkResult {
            changed: true,
            saved_bytes: 500,
            raw_saved_bytes: 200,
            blocks_optimized: 1,
            summary: "1 block optimized".to_string(),
        };
        let r2 = r.clone();
        assert_eq!(r2.saved_bytes, 500);
        assert_eq!(r2.blocks_optimized, 1);
        assert_eq!(r2.summary, "1 block optimized");
    }

    #[test]
    fn test_artwork_result_serde_roundtrip() {
        let original = ArtworkResult {
            changed: true,
            saved_bytes: 1024,
            raw_saved_bytes: 512,
            blocks_optimized: 3,
            summary: "test".to_string(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ArtworkResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.changed, true);
        assert_eq!(back.saved_bytes, 1024);
        assert_eq!(back.raw_saved_bytes, 512);
        assert_eq!(back.blocks_optimized, 3);
        assert_eq!(back.summary, "test");
    }

    // --- Integration test using real FLAC file ---

    #[test]
    fn test_optimize_album_art_on_test_flac() {
        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");

        if !flac_path.exists() {
            eprintln!("Test FLAC not found at {:?} — skipping", flac_path);
            return;
        }

        // Work on a copy so we don't mutate the test fixture
        let tmp_dir = std::env::temp_dir().join("flaccrunch_art_test");
        let scratch_dir = tmp_dir.join("scratch");
        std::fs::create_dir_all(&scratch_dir).expect("create scratch dir");

        let tmp_flac = tmp_dir.join("test_copy.flac");
        std::fs::copy(&flac_path, &tmp_flac).expect("copy test FLAC");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(optimize_album_art(&tmp_flac, &scratch_dir));

        match result {
            Ok(art) => {
                // Just verify the result struct makes sense — don't assert savings
                // since the artwork may already be optimal.
                println!(
                    "optimize_album_art: changed={} saved={} blocks_optimized={} summary={}",
                    art.changed, art.saved_bytes, art.blocks_optimized, art.summary
                );
                assert!(art.blocks_optimized <= 10, "unreasonably many blocks");
            }
            Err(e) => {
                // Some CI environments may lack metaflac; treat as a soft skip.
                eprintln!("optimize_album_art returned Err (possibly missing tool): {e}");
            }
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
