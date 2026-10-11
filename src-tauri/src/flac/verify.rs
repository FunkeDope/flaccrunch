//! Post-encode proof that nothing but the compression changed.
//!
//! The audio is covered by the PRE/OUT MD5 comparison in the pipeline. This
//! module covers everything else: tags (byte-exact, vendor string included),
//! pictures (same header fields, same decoded pixels, same embedded metadata
//! segments), application and cuesheet blocks (byte-exact), stream parameters
//! and any non-FLAC prefix (ID3v2). A mismatch fails the file and the
//! original is kept.

use crate::flac::ffi::path_cstring;
use crate::flac::metadata::{extract_non_flac_prefix, MetadataChain, VorbisCommentBlock};
use crate::image::{decode_image, DecodedImage};
use libflac_sys::*;
use std::ffi::CStr;
use std::path::Path;

/// Header fields of a PICTURE block (everything except the image bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PictureHeader {
    kind: u32,
    mime: Vec<u8>,
    description: Vec<u8>,
    width: u32,
    height: u32,
    depth: u32,
    colors: u32,
}

struct Picture {
    header: PictureHeader,
    data: Vec<u8>,
}

/// Everything we compare, pulled out of one chain.
struct Snapshot {
    stream_info: (u32, u32, u32, u64),
    comments: Vec<VorbisCommentBlock>,
    pictures: Vec<Picture>,
    /// Raw pointers stay valid only while `chain` lives.
    applications: Vec<*mut FLAC__StreamMetadata>,
    cuesheets: Vec<*mut FLAC__StreamMetadata>,
    _chain: MetadataChain,
}

unsafe fn bytes(ptr: *const u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(ptr, len).to_vec()
    }
}

fn snapshot(path: &Path) -> Result<Snapshot, String> {
    let path_c = path_cstring(path)?;
    let stream_info = unsafe {
        let mut m: FLAC__StreamMetadata = std::mem::zeroed();
        if FLAC__metadata_get_streaminfo(path_c.as_ptr(), &mut m) == 0 {
            return Err("cannot read STREAMINFO".to_string());
        }
        let si = m.data.stream_info;
        (
            si.sample_rate,
            si.channels,
            si.bits_per_sample,
            si.total_samples,
        )
    };

    let chain = MetadataChain::read(path)?;
    let mut comments = Vec::new();
    let mut pictures = Vec::new();
    let mut applications = Vec::new();
    let mut cuesheets = Vec::new();
    {
        let iter = chain.iter()?;
        loop {
            let block = iter.block();
            match iter.block_type() {
                FLAC__METADATA_TYPE_VORBIS_COMMENT => {
                    comments.push(unsafe { VorbisCommentBlock::from_raw(block) });
                }
                FLAC__METADATA_TYPE_PICTURE => unsafe {
                    let p = &(*block).data.picture;
                    pictures.push(Picture {
                        header: PictureHeader {
                            kind: p.type_,
                            mime: if p.mime_type.is_null() {
                                Vec::new()
                            } else {
                                CStr::from_ptr(p.mime_type).to_bytes().to_vec()
                            },
                            description: if p.description.is_null() {
                                Vec::new()
                            } else {
                                CStr::from_ptr(p.description as *const _)
                                    .to_bytes()
                                    .to_vec()
                            },
                            width: p.width,
                            height: p.height,
                            depth: p.depth,
                            colors: p.colors,
                        },
                        data: bytes(p.data, p.data_length as usize),
                    });
                },
                FLAC__METADATA_TYPE_APPLICATION => applications.push(block),
                FLAC__METADATA_TYPE_CUESHEET => cuesheets.push(block),
                _ => {}
            }
            if !iter.next() {
                break;
            }
        }
    }
    Ok(Snapshot {
        stream_info,
        comments,
        pictures,
        applications,
        cuesheets,
        _chain: chain,
    })
}

/// Deep-compare two blocks' contents. `FLAC__metadata_object_is_equal` also
/// compares the `is_last` flag, which legitimately changes when trailing
/// PADDING is removed, so compare normalized clones instead.
///
/// # Safety
/// Both pointers must be valid metadata objects.
unsafe fn blocks_equal_ignoring_position(
    a: *mut FLAC__StreamMetadata,
    b: *mut FLAC__StreamMetadata,
) -> bool {
    let ca = FLAC__metadata_object_clone(a);
    let cb = FLAC__metadata_object_clone(b);
    let equal = if ca.is_null() || cb.is_null() {
        false
    } else {
        (*ca).is_last = 0;
        (*cb).is_last = 0;
        FLAC__metadata_object_is_equal(ca, cb) != 0
    };
    if !ca.is_null() {
        FLAC__metadata_object_delete(ca);
    }
    if !cb.is_null() {
        FLAC__metadata_object_delete(cb);
    }
    equal
}

const MARKER_PREFIX: &[u8] = b"FLACCRUNCH_INFO=";

fn without_marker(blocks: &[VorbisCommentBlock]) -> Vec<VorbisCommentBlock> {
    blocks
        .iter()
        .map(|b| VorbisCommentBlock {
            vendor: b.vendor.clone(),
            entries: b
                .entries
                .iter()
                .filter(|e| !e.to_ascii_uppercase().starts_with(MARKER_PREFIX))
                .cloned()
                .collect(),
        })
        .collect()
}

/// Verify that `out` carries exactly the same data as `src`, apart from the
/// compression of audio and pictures (and, when `marker_allowed`, one added
/// or refreshed FLACCRUNCH_INFO tag). `Err` describes the first mismatch.
pub fn verify_metadata_equivalent(
    src: &Path,
    out: &Path,
    marker_allowed: bool,
) -> Result<(), String> {
    let a = snapshot(src).map_err(|e| format!("source: {e}"))?;
    let b = snapshot(out).map_err(|e| format!("output: {e}"))?;

    if a.stream_info != b.stream_info {
        return Err(format!(
            "stream parameters differ: {:?} vs {:?}",
            a.stream_info, b.stream_info
        ));
    }

    let (ca, cb) = if marker_allowed {
        (without_marker(&a.comments), without_marker(&b.comments))
    } else {
        (a.comments.clone(), b.comments.clone())
    };
    if ca.len() != cb.len() {
        return Err(format!(
            "VORBIS_COMMENT block count differs: {} vs {}",
            ca.len(),
            cb.len()
        ));
    }
    for (i, (x, y)) in ca.iter().zip(&cb).enumerate() {
        if x.vendor != y.vendor {
            return Err(format!("vendor string differs in VORBIS_COMMENT #{i}"));
        }
        if x.entries != y.entries {
            return Err(format!("tags differ in VORBIS_COMMENT #{i}"));
        }
    }

    if a.pictures.len() != b.pictures.len() {
        return Err(format!(
            "PICTURE block count differs: {} vs {}",
            a.pictures.len(),
            b.pictures.len()
        ));
    }
    for (i, (x, y)) in a.pictures.iter().zip(&b.pictures).enumerate() {
        if x.header != y.header {
            return Err(format!("PICTURE #{i} header differs"));
        }
        if x.data == y.data {
            continue;
        }
        if y.data.len() > x.data.len() {
            return Err(format!("PICTURE #{i} grew"));
        }
        match (decode_image(&x.data), decode_image(&y.data)) {
            (Some(p), Some(q)) if p == q => {}
            (Some(_), Some(_)) => return Err(format!("PICTURE #{i} pixels differ")),
            _ => {
                return Err(format!(
                    "PICTURE #{i} changed but cannot be decoded for comparison"
                ))
            }
        }
        if let (DecodedImage::Jpeg(..), true) = (
            decode_image(&x.data).unwrap(),
            crate::image::jpeg::metadata_segments(&x.data)
                != crate::image::jpeg::metadata_segments(&y.data),
        ) {
            return Err(format!("PICTURE #{i} JPEG metadata segments differ"));
        }
    }

    for (name, xs, ys) in [
        ("APPLICATION", &a.applications, &b.applications),
        ("CUESHEET", &a.cuesheets, &b.cuesheets),
    ] {
        if xs.len() != ys.len() {
            return Err(format!(
                "{name} block count differs: {} vs {}",
                xs.len(),
                ys.len()
            ));
        }
        for (i, (&x, &y)) in xs.iter().zip(ys).enumerate() {
            if !unsafe { blocks_equal_ignoring_position(x, y) } {
                return Err(format!("{name} #{i} differs"));
            }
        }
    }

    let pa = extract_non_flac_prefix(src).unwrap_or_default();
    let pb = extract_non_flac_prefix(out).unwrap_or_default();
    if pa != pb {
        return Err("non-FLAC prefix (ID3v2) differs".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flac::encoder::{encode_flac, EncodeOptions};
    use crate::flac::marker::write_crunched_marker;
    use crate::test_util::{plant_illegal_comment, small_flac};

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    #[test]
    fn test_identical_file_is_equivalent() {
        let dir = tempfile::tempdir().unwrap();
        let a = small_flac(dir.path(), "a.flac", 1);
        assert_eq!(verify_metadata_equivalent(&a, &a, false), Ok(()));
    }

    #[test]
    fn test_full_pipeline_output_is_equivalent() {
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 1);
        plant_illegal_comment(&src, b"WEIRD=\xff\xfe");
        let out = dir.path().join("out.flac");
        rt().block_on(encode_flac(
            &src,
            &out,
            EncodeOptions::default(),
            None,
            None,
        ))
        .unwrap();
        rt().block_on(crate::artwork::optimize::optimize_album_art(&out))
            .unwrap();
        assert_eq!(verify_metadata_equivalent(&src, &out, false), Ok(()));
    }

    #[test]
    fn test_marker_is_only_tolerated_when_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 1);
        let out = dir.path().join("out.flac");
        std::fs::copy(&src, &out).unwrap();
        rt().block_on(write_crunched_marker(&out)).unwrap();
        assert!(verify_metadata_equivalent(&src, &out, false).is_err());
        assert_eq!(verify_metadata_equivalent(&src, &out, true), Ok(()));
    }

    #[test]
    fn test_changed_tag_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let src = small_flac(dir.path(), "src.flac", 1);
        let out = dir.path().join("out.flac");
        std::fs::copy(&src, &out).unwrap();
        plant_illegal_comment(&out, b"EXTRA=tag");
        let err = verify_metadata_equivalent(&src, &out, false).unwrap_err();
        assert!(err.contains("tags differ"), "{err}");
    }

    #[test]
    fn test_different_audio_parameters_detected() {
        let dir = tempfile::tempdir().unwrap();
        let a = small_flac(dir.path(), "a.flac", 1);
        let b = small_flac(dir.path(), "b.flac", 2);
        let err = verify_metadata_equivalent(&a, &b, false).unwrap_err();
        assert!(err.contains("stream parameters"), "{err}");
    }
}
