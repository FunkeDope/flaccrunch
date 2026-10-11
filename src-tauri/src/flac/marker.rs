//! The `FLACCRUNCH_INFO` Vorbis-comment marker that tags files already
//! processed by FlacCrunch.

use crate::flac::metadata::{MetadataBlocks, MetadataChain};
use libflac_sys::*;
use serde_json::json;
use std::ffi::{CStr, CString};
use std::path::Path;

/// Vorbis-comment field name used to mark a file as already crunched.
/// Value of the entry is a JSON object with version + timestamp metadata.
pub const CRUNCHED_TAG: &str = "FLACCRUNCH_INFO";

/// Read libFLAC's compile-time version string at runtime via the exported
/// `FLAC__VERSION_STRING` C global.
pub fn libflac_version() -> String {
    unsafe {
        let ptr = FLAC__VERSION_STRING;
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

/// Build the JSON value embedded in the FLACCRUNCH_INFO comment.
fn build_marker_value() -> String {
    json!({
        "crunched": true,
        "flaccrunchVersion": env!("CARGO_PKG_VERSION"),
        "libflacVersion": libflac_version(),
        "crunchedAt": chrono::Utc::now().to_rfc3339(),
    })
    .to_string()
}

/// Add (or replace in place) the FLACCRUNCH_INFO entry on a VORBIS_COMMENT
/// block object.
///
/// Non-destructive guarantees:
/// - Only the entry whose field name is `FLACCRUNCH_INFO` is added/modified.
/// - All other comments retain their original index, name, and value.
/// - The vendor string is not touched.
///
/// # Safety
/// `vc_block` must point to a valid VORBIS_COMMENT metadata object.
unsafe fn upsert_marker_entry(vc_block: *mut FLAC__StreamMetadata) -> Result<(), String> {
    let tag_c = CString::new(CRUNCHED_TAG).map_err(|_| "Invalid tag name")?;
    let value_c = CString::new(build_marker_value()).map_err(|_| "Invalid marker value")?;

    let mut entry: FLAC__StreamMetadata_VorbisComment_Entry = std::mem::zeroed();
    if FLAC__metadata_object_vorbiscomment_entry_from_name_value_pair(
        &mut entry,
        tag_c.as_ptr(),
        value_c.as_ptr(),
    ) == 0
    {
        return Err("Failed to build Vorbis comment entry".to_string());
    }

    // find_entry_from is case-insensitive on the field name (per Vorbis spec).
    let existing = FLAC__metadata_object_vorbiscomment_find_entry_from(vc_block, 0, tag_c.as_ptr());
    let ok = if existing >= 0 {
        FLAC__metadata_object_vorbiscomment_set_comment(vc_block, existing as u32, entry, 1)
    } else {
        FLAC__metadata_object_vorbiscomment_append_comment(vc_block, entry, 1)
    };

    // set/append with copy=1 duplicate the entry; free our malloc'd buffer.
    if !entry.entry.is_null() {
        libc::free(entry.entry as *mut libc::c_void);
    }

    if ok == 0 {
        return Err("Failed to set/append Vorbis comment".to_string());
    }
    Ok(())
}

/// Ensure `blocks` (metadata about to be attached to an encoder) contains a
/// VORBIS_COMMENT block carrying the marker. Creates the block if the source
/// file had none.
pub fn ensure_marker_in_blocks(blocks: &mut MetadataBlocks) -> Result<(), String> {
    let vc = match blocks.find(FLAC__METADATA_TYPE_VORBIS_COMMENT) {
        Some(vc) => vc,
        None => {
            let vc = unsafe { FLAC__metadata_object_new(FLAC__METADATA_TYPE_VORBIS_COMMENT) };
            if vc.is_null() {
                return Err("Failed to create VORBIS_COMMENT block".to_string());
            }
            // Conventional layout: tags before pictures.
            blocks.insert(0, vc);
            vc
        }
    };
    unsafe { upsert_marker_entry(vc) }
}

/// Append (or update in place) the FLACCRUNCH_INFO Vorbis comment on an
/// existing FLAC file. Rewrites the file's metadata; audio is untouched.
pub async fn write_crunched_marker(flac_path: &Path) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || write_crunched_marker_native(&flac_path))
        .await
        .map_err(|e| format!("Marker write task panicked: {e}"))?
}

fn write_crunched_marker_native(flac_path: &Path) -> Result<(), String> {
    let chain = MetadataChain::read(flac_path)?;
    let iter = chain.iter()?;

    let vc_block = match iter.find(FLAC__METADATA_TYPE_VORBIS_COMMENT) {
        Some(block) => block,
        None => {
            // No Vorbis-comment block: create one right after STREAMINFO.
            let new_block =
                unsafe { FLAC__metadata_object_new(FLAC__METADATA_TYPE_VORBIS_COMMENT) };
            if new_block.is_null() {
                return Err("Failed to create VORBIS_COMMENT block".to_string());
            }
            iter.reset();
            // SAFETY: `new_block` is a fresh, unowned libFLAC object.
            if !unsafe { iter.insert_block_after(new_block) } {
                unsafe { FLAC__metadata_object_delete(new_block) };
                return Err("Failed to insert VORBIS_COMMENT block".to_string());
            }
            new_block
        }
    };

    unsafe { upsert_marker_entry(vc_block)? };
    drop(iter);

    // sort_padding only consolidates PADDING blocks; it does not reorder
    // Vorbis comments or other metadata blocks.
    chain.sort_padding();
    chain.write(true)
}

/// Return true iff the file carries a `FLACCRUNCH_INFO` Vorbis comment.
/// Presence alone is sufficient — the JSON value is not parsed.
pub fn read_crunched_marker(flac_path: &Path) -> Result<bool, String> {
    let tag_c = CString::new(CRUNCHED_TAG).map_err(|_| "Invalid tag name")?;
    let chain = MetadataChain::read(flac_path)?;
    let iter = chain.iter()?;
    let Some(vc) = iter.find(FLAC__METADATA_TYPE_VORBIS_COMMENT) else {
        return Ok(false);
    };
    let idx = unsafe { FLAC__metadata_object_vorbiscomment_find_entry_from(vc, 0, tag_c.as_ptr()) };
    Ok(idx >= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_libflac_version_nonempty() {
        assert!(!libflac_version().is_empty());
    }

    #[test]
    fn test_marker_value_contains_expected_keys() {
        let s = build_marker_value();
        assert!(s.contains("\"crunched\":true"));
        assert!(s.contains("\"flaccrunchVersion\""));
        assert!(s.contains("\"libflacVersion\""));
        assert!(s.contains("\"crunchedAt\""));
    }

    #[test]
    fn test_ensure_marker_creates_vorbis_comment_when_absent() {
        let mut blocks = MetadataBlocks::new();
        ensure_marker_in_blocks(&mut blocks).unwrap();
        assert_eq!(blocks.len(), 1);
        let vc = blocks.find(FLAC__METADATA_TYPE_VORBIS_COMMENT).unwrap();
        let tag = CString::new(CRUNCHED_TAG).unwrap();
        let idx =
            unsafe { FLAC__metadata_object_vorbiscomment_find_entry_from(vc, 0, tag.as_ptr()) };
        assert_eq!(idx, 0);

        // Second call updates in place: still one comment entry.
        ensure_marker_in_blocks(&mut blocks).unwrap();
        assert_eq!(blocks.len(), 1);
        let n = unsafe { (*vc).data.vorbis_comment.num_comments };
        assert_eq!(n, 1);
    }

    /// Path to the shared test FLAC fixture (~45 MB). Skips gracefully if the
    /// fixture is not available.
    fn test_fixture_path() -> Option<std::path::PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        p.exists().then_some(p)
    }

    /// Read all Vorbis comment entries (FIELD=value strings) from a FLAC file.
    pub(crate) fn read_all_comments(path: &Path) -> Vec<String> {
        let chain = MetadataChain::read(path).unwrap();
        let iter = chain.iter().unwrap();
        let mut out = Vec::new();
        if let Some(block) = iter.find(FLAC__METADATA_TYPE_VORBIS_COMMENT) {
            unsafe {
                let vc = &(*block).data.vorbis_comment;
                for i in 0..vc.num_comments as isize {
                    let entry = *vc.comments.offset(i);
                    let bytes = std::slice::from_raw_parts(entry.entry, entry.length as usize);
                    out.push(String::from_utf8_lossy(bytes).into_owned());
                }
            }
        }
        out
    }

    /// Read the Vorbis vendor string.
    fn read_vendor_string(path: &Path) -> String {
        let chain = MetadataChain::read(path).unwrap();
        let iter = chain.iter().unwrap();
        let block = iter.find(FLAC__METADATA_TYPE_VORBIS_COMMENT).unwrap();
        unsafe {
            let vc = &(*block).data.vorbis_comment;
            let bytes = std::slice::from_raw_parts(
                vc.vendor_string.entry,
                vc.vendor_string.length as usize,
            );
            String::from_utf8_lossy(bytes).into_owned()
        }
    }

    /// Replace all Vorbis comments so each test starts from a known baseline.
    pub(crate) fn set_comments(path: &Path, comments: &[(&str, &str)]) {
        let chain = MetadataChain::read(path).unwrap();
        let iter = chain.iter().unwrap();
        let vc_block = iter
            .find(FLAC__METADATA_TYPE_VORBIS_COMMENT)
            .expect("fixture must have a vorbis comment block");
        unsafe {
            FLAC__metadata_object_vorbiscomment_resize_comments(vc_block, 0);
            for (name, value) in comments {
                let n = CString::new(*name).unwrap();
                let v = CString::new(*value).unwrap();
                let mut entry: FLAC__StreamMetadata_VorbisComment_Entry = std::mem::zeroed();
                assert_ne!(
                    FLAC__metadata_object_vorbiscomment_entry_from_name_value_pair(
                        &mut entry,
                        n.as_ptr(),
                        v.as_ptr(),
                    ),
                    0
                );
                assert_ne!(
                    FLAC__metadata_object_vorbiscomment_append_comment(vc_block, entry, 1),
                    0
                );
                if !entry.entry.is_null() {
                    libc::free(entry.entry as *mut libc::c_void);
                }
            }
        }
        drop(iter);
        chain.sort_padding();
        chain.write(true).unwrap();
    }

    #[tokio::test]
    async fn test_marker_round_trip_and_non_destructive() {
        let Some(src) = test_fixture_path() else {
            eprintln!("un-optimized.flac fixture missing — skipping");
            return;
        };

        let temp = tempfile::tempdir().unwrap();
        let copy = temp.path().join("marker_test.flac");
        std::fs::copy(&src, &copy).expect("copy fixture");

        set_comments(
            &copy,
            &[("ARTIST", "foo"), ("TITLE", "bar"), ("ALBUM", "baz")],
        );
        let vendor_before = read_vendor_string(&copy);

        assert!(!read_crunched_marker(&copy).unwrap());

        write_crunched_marker(&copy).await.expect("first write");
        let after_first = read_all_comments(&copy);
        assert_eq!(after_first.len(), 4, "{after_first:?}");
        assert!(after_first[0].starts_with("ARTIST=foo"));
        assert!(after_first[1].starts_with("TITLE=bar"));
        assert!(after_first[2].starts_with("ALBUM=baz"));
        assert!(after_first[3].starts_with("FLACCRUNCH_INFO="));
        assert!(after_first[3].contains("\"crunched\":true"));
        assert!(read_crunched_marker(&copy).unwrap());
        assert_eq!(vendor_before, read_vendor_string(&copy));

        std::thread::sleep(std::time::Duration::from_millis(20));

        write_crunched_marker(&copy).await.expect("second write");
        let after_second = read_all_comments(&copy);
        assert_eq!(after_second.len(), 4, "{after_second:?}");
        assert_eq!(after_second[0], "ARTIST=foo");
        assert_eq!(after_second[1], "TITLE=bar");
        assert_eq!(after_second[2], "ALBUM=baz");
        assert!(after_second[3].starts_with("FLACCRUNCH_INFO="));
        assert_ne!(after_first[3], after_second[3], "timestamp refreshes");
        assert_eq!(vendor_before, read_vendor_string(&copy));
    }

    #[tokio::test]
    async fn test_read_marker_returns_false_when_absent() {
        let Some(src) = test_fixture_path() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let copy = temp.path().join("no_marker.flac");
        std::fs::copy(&src, &copy).unwrap();
        set_comments(&copy, &[("ARTIST", "foo")]);
        assert!(!read_crunched_marker(&copy).unwrap());
    }
}
