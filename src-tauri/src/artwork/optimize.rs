//! Embedded artwork optimization + PADDING removal, done in memory on the
//! metadata chain in a single read/write pass.
//!
//! The previous implementation exported every picture to a shared scratch
//! directory, optimized it on disk, removed all PICTURE blocks and re-imported
//! them — each of those steps rewrote the whole FLAC file, and because the
//! scratch file names were derived from the picture index alone, two workers
//! in the artwork stage at the same time could overwrite each other's export
//! and swap cover art between files.

use crate::flac::metadata::MetadataChain;
use crate::image::detect::{detect_image_format, ImageFormat};
use crate::image::jpeg::optimize_jpeg;
use crate::image::png::optimize_png;
use libflac_sys::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Result of album art optimization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtworkResult {
    pub changed: bool,
    /// Net bytes the file shrank by in this stage (pictures + padding).
    pub saved_bytes: i64,
    /// Bytes saved inside PICTURE blocks only.
    pub raw_saved_bytes: i64,
    /// Bytes of PADDING blocks removed.
    pub padding_saved_bytes: i64,
    pub blocks_optimized: u32,
    pub summary: String,
}

/// Optimize all embedded album art in `flac_path` (losslessly) and strip
/// PADDING blocks.
pub async fn optimize_album_art(flac_path: &Path) -> Result<ArtworkResult, String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || optimize_album_art_native(&flac_path))
        .await
        .map_err(|e| format!("Artwork task panicked: {e}"))?
}

/// Try the optimizers on raw picture bytes. Returns a replacement only when
/// it is strictly smaller.
pub fn optimize_picture_bytes(data: &[u8]) -> Option<Vec<u8>> {
    let candidate = match detect_image_format(data)? {
        ImageFormat::Png => optimize_png(data).ok()?,
        ImageFormat::Jpeg => optimize_jpeg(data)?,
    };
    (candidate.len() < data.len()).then_some(candidate)
}

fn optimize_album_art_native(flac_path: &Path) -> Result<ArtworkResult, String> {
    let size_before = fs::metadata(flac_path)
        .map_err(|e| format!("Failed to read FLAC metadata: {e}"))?
        .len();

    let chain = MetadataChain::read(flac_path)?;
    let iter = chain.iter()?;

    let mut raw_saved: i64 = 0;
    let mut padding_saved: i64 = 0;
    let mut blocks_optimized: u32 = 0;
    let mut changed = false;

    loop {
        let block = iter.block();
        match iter.block_type() {
            FLAC__METADATA_TYPE_PICTURE => unsafe {
                let pic = &(*block).data.picture;
                let is_url = !pic.mime_type.is_null()
                    && std::ffi::CStr::from_ptr(pic.mime_type).to_bytes() == b"-->";
                if !is_url && !pic.data.is_null() && pic.data_length > 0 {
                    let data = std::slice::from_raw_parts(pic.data, pic.data_length as usize);
                    if let Some(optimized) = optimize_picture_bytes(data) {
                        let before = data.len() as i64;
                        // copy=1: libFLAC duplicates the buffer; `optimized`
                        // can be dropped afterwards.
                        if FLAC__metadata_object_picture_set_data(
                            block,
                            optimized.as_ptr() as *mut FLAC__byte,
                            optimized.len() as u32,
                            1,
                        ) != 0
                        {
                            raw_saved += before - optimized.len() as i64;
                            blocks_optimized += 1;
                            changed = true;
                        }
                    }
                }
            },
            FLAC__METADATA_TYPE_PADDING => {
                // Body length + the 4-byte block header that goes with it.
                padding_saved +=
                    unsafe { (*block).length } as i64 + FLAC__STREAM_METADATA_HEADER_LENGTH as i64;
                // delete_block leaves the iterator on the previous block; the
                // following next() lands on the block that came after the
                // deleted one, so every block is visited exactly once.
                if iter.delete_block(false) {
                    changed = true;
                    if !iter.next() {
                        break;
                    }
                    continue;
                }
            }
            _ => {}
        }
        if !iter.next() {
            break;
        }
    }
    drop(iter);

    if changed {
        // No padding left to absorb the change: libFLAC rewrites the file.
        chain.write(false)?;
    }

    let size_after = fs::metadata(flac_path)
        .map_err(|e| format!("Failed to read FLAC size after art optimization: {e}"))?
        .len();
    let net_saved = size_before as i64 - size_after as i64;

    Ok(ArtworkResult {
        changed,
        saved_bytes: net_saved,
        raw_saved_bytes: raw_saved,
        padding_saved_bytes: padding_saved,
        blocks_optimized,
        summary: format!(
            "{blocks_optimized} picture(s) optimized ({raw_saved} B), padding removed ({padding_saved} B), net {net_saved} B"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artwork_result_default() {
        let r = ArtworkResult::default();
        assert!(!r.changed);
        assert_eq!(r.saved_bytes, 0);
        assert_eq!(r.padding_saved_bytes, 0);
        assert_eq!(r.blocks_optimized, 0);
    }

    #[test]
    fn test_artwork_result_serde_roundtrip() {
        let original = ArtworkResult {
            changed: true,
            saved_bytes: 1024,
            raw_saved_bytes: 512,
            padding_saved_bytes: 512,
            blocks_optimized: 3,
            summary: "test".to_string(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ArtworkResult = serde_json::from_str(&json).expect("deserialize");
        assert!(back.changed);
        assert_eq!(back.saved_bytes, 1024);
        assert_eq!(back.padding_saved_bytes, 512);
        assert_eq!(back.blocks_optimized, 3);
    }

    #[test]
    fn test_optimize_picture_bytes_unknown_format_is_none() {
        assert!(optimize_picture_bytes(b"BM\0\0\0\0\0\0").is_none());
        assert!(optimize_picture_bytes(b"").is_none());
    }

    #[test]
    fn test_optimize_picture_bytes_strips_jpeg_exif() {
        // JPEG with an APP1 segment: pass 1 removes it, so the result shrinks.
        let jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xE1, 0x00, 0x08, b'E', b'x', b'i', b'f',
            0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xAB, 0xCD,
            0xFF, 0xD9,
        ];
        let out = optimize_picture_bytes(&jpeg).expect("should shrink");
        assert!(out.len() < jpeg.len());
    }

    #[test]
    fn test_optimize_album_art_missing_file_errors() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(optimize_album_art(Path::new("/nope/none.flac")))
            .is_err());
    }

    /// Integration test on the real fixture: artwork + padding handled in one
    /// pass, audio untouched, pictures still present afterwards.
    #[test]
    fn test_optimize_album_art_on_test_flac() {
        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        if !flac_path.exists() {
            eprintln!("Test FLAC not found — skipping");
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        let copy = tmp.path().join("test_copy.flac");
        std::fs::copy(&flac_path, &copy).expect("copy test FLAC");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let before_hash = rt
            .block_on(crate::flac::hasher::hash_decoded_audio(&copy))
            .unwrap();
        let pictures_before = {
            let chain = MetadataChain::read(&copy).unwrap();
            let it = chain.iter().unwrap();
            let mut n = 0;
            loop {
                if it.block_type() == FLAC__METADATA_TYPE_PICTURE {
                    n += 1;
                }
                if !it.next() {
                    break;
                }
            }
            n
        };

        let art = rt.block_on(optimize_album_art(&copy)).expect("optimize");
        println!("{}", art.summary);
        assert!(art.saved_bytes >= 0);
        assert_eq!(
            art.saved_bytes,
            art.raw_saved_bytes + art.padding_saved_bytes,
            "net savings must equal picture + padding savings (no re-import overhead)"
        );

        let after_hash = rt
            .block_on(crate::flac::hasher::hash_decoded_audio(&copy))
            .unwrap();
        assert_eq!(before_hash, after_hash, "audio must be untouched");

        let chain = MetadataChain::read(&copy).unwrap();
        let it = chain.iter().unwrap();
        let mut pictures_after = 0;
        let mut padding_after = 0;
        loop {
            match it.block_type() {
                FLAC__METADATA_TYPE_PICTURE => pictures_after += 1,
                FLAC__METADATA_TYPE_PADDING => padding_after += 1,
                _ => {}
            }
            if !it.next() {
                break;
            }
        }
        assert_eq!(
            pictures_before, pictures_after,
            "pictures must be preserved"
        );
        assert_eq!(padding_after, 0, "all padding must be gone");

        // Idempotent: a second pass finds nothing to do.
        let again = rt.block_on(optimize_album_art(&copy)).unwrap();
        assert!(!again.changed);
        assert_eq!(again.saved_bytes, 0);
    }
}
