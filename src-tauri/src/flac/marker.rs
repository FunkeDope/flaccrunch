use serde_json::json;
use std::ffi::{CStr, CString};
use std::path::Path;

/// Vorbis-comment field name used to mark a file as already crunched.
/// Value of the entry is a JSON object with version + timestamp metadata.
pub const CRUNCHED_TAG: &str = "FLACCRUNCH_INFO";

/// Read libFLAC's compile-time version string at runtime via the exported
/// `FLAC__VERSION_STRING` C global.
pub fn libflac_version() -> String {
    use libflac_sys::FLAC__VERSION_STRING;
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
    let payload = json!({
        "crunched": true,
        "flaccrunchVersion": env!("CARGO_PKG_VERSION"),
        "libflacVersion": libflac_version(),
        "crunchedAt": chrono::Utc::now().to_rfc3339(),
    });
    payload.to_string()
}

/// Append (or update in place) the FLACCRUNCH_INFO Vorbis comment on a FLAC file.
///
/// Non-destructive guarantees:
/// - Only one Vorbis-comment entry is added or modified: the one whose field
///   name is `FLACCRUNCH_INFO`.
/// - All other comments retain their original index, name, and value.
/// - The Vorbis vendor string is not touched.
/// - PICTURE / APPLICATION / CUESHEET / SEEKTABLE / PADDING / STREAMINFO
///   blocks are not touched (we never call any function that mutates them).
pub async fn write_crunched_marker(flac_path: &Path) -> Result<(), String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || write_crunched_marker_native(&flac_path))
        .await
        .map_err(|e| format!("Marker write task panicked: {e}"))?
}

fn write_crunched_marker_native(flac_path: &Path) -> Result<(), String> {
    use libflac_sys::*;

    let path_cstr =
        CString::new(flac_path.to_string_lossy().as_bytes()).map_err(|_| "Invalid path")?;
    let tag_cstr = CString::new(CRUNCHED_TAG).map_err(|_| "Invalid tag name")?;
    let value_str = build_marker_value();
    let value_cstr = CString::new(value_str).map_err(|_| "Invalid marker value")?;

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

        // Locate (or create) a VORBIS_COMMENT block.
        let mut vc_block: *mut FLAC__StreamMetadata = std::ptr::null_mut();
        loop {
            let block = FLAC__metadata_iterator_get_block(iter);
            if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_VORBIS_COMMENT {
                vc_block = block;
                break;
            }
            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        if vc_block.is_null() {
            // No Vorbis-comment block: create one and insert it after STREAMINFO.
            let new_block = FLAC__metadata_object_new(FLAC__METADATA_TYPE_VORBIS_COMMENT);
            if new_block.is_null() {
                FLAC__metadata_iterator_delete(iter);
                FLAC__metadata_chain_delete(chain);
                return Err("Failed to create VORBIS_COMMENT block".to_string());
            }
            // Reset iterator to the start (STREAMINFO), then insert new block after it.
            FLAC__metadata_iterator_init(iter, chain);
            if FLAC__metadata_iterator_insert_block_after(iter, new_block) == 0 {
                FLAC__metadata_object_delete(new_block);
                FLAC__metadata_iterator_delete(iter);
                FLAC__metadata_chain_delete(chain);
                return Err("Failed to insert VORBIS_COMMENT block".to_string());
            }
            vc_block = new_block;
        }

        // Build the entry: "FLACCRUNCH_INFO=<json>"
        let mut entry: FLAC__StreamMetadata_VorbisComment_Entry = std::mem::zeroed();
        if FLAC__metadata_object_vorbiscomment_entry_from_name_value_pair(
            &mut entry,
            tag_cstr.as_ptr(),
            value_cstr.as_ptr(),
        ) == 0
        {
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to build Vorbis comment entry".to_string());
        }

        // find_entry_from is case-insensitive on the field name (per Vorbis spec).
        let existing_index =
            FLAC__metadata_object_vorbiscomment_find_entry_from(vc_block, 0, tag_cstr.as_ptr());

        let ok = if existing_index >= 0 {
            // Replace ONLY the existing entry in place — surrounding entries unchanged.
            FLAC__metadata_object_vorbiscomment_set_comment(
                vc_block,
                existing_index as u32,
                entry,
                1, // copy
            )
        } else {
            // Append a single new entry; existing entries keep their indices.
            FLAC__metadata_object_vorbiscomment_append_comment(vc_block, entry, 1 /* copy */)
        };

        // entry's `entry` pointer is owned by the malloc'd buffer from
        // entry_from_name_value_pair. set_comment / append_comment with copy=1
        // duplicate it, so we must free our local copy.
        if !entry.entry.is_null() {
            libc::free(entry.entry as *mut libc::c_void);
        }

        if ok == 0 {
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to set/append Vorbis comment".to_string());
        }

        FLAC__metadata_iterator_delete(iter);

        // sort_padding only consolidates PADDING blocks; it does not reorder
        // Vorbis comments or other metadata blocks.
        FLAC__metadata_chain_sort_padding(chain);
        if FLAC__metadata_chain_write(chain, 1, 0) == 0 {
            FLAC__metadata_chain_delete(chain);
            return Err("Failed to write metadata chain".to_string());
        }

        FLAC__metadata_chain_delete(chain);
    }

    Ok(())
}

/// Return true iff the file carries a `FLACCRUNCH_INFO` Vorbis comment.
/// Presence alone is sufficient — the JSON value is not parsed.
pub fn read_crunched_marker(flac_path: &Path) -> Result<bool, String> {
    use libflac_sys::*;

    let path_cstr =
        CString::new(flac_path.to_string_lossy().as_bytes()).map_err(|_| "Invalid path")?;
    let tag_cstr = CString::new(CRUNCHED_TAG).map_err(|_| "Invalid tag name")?;

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

        let mut found = false;
        loop {
            let block = FLAC__metadata_iterator_get_block(iter);
            if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_VORBIS_COMMENT {
                let idx = FLAC__metadata_object_vorbiscomment_find_entry_from(
                    block,
                    0,
                    tag_cstr.as_ptr(),
                );
                if idx >= 0 {
                    found = true;
                    break;
                }
            }
            if FLAC__metadata_iterator_next(iter) == 0 {
                break;
            }
        }

        FLAC__metadata_iterator_delete(iter);
        FLAC__metadata_chain_delete(chain);

        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_libflac_version_nonempty() {
        let v = libflac_version();
        assert!(!v.is_empty(), "FLAC__VERSION_STRING should be readable");
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
    fn test_crunched_tag_constant() {
        assert_eq!(CRUNCHED_TAG, "FLACCRUNCH_INFO");
    }

    /// Path to the shared test FLAC fixture (~45 MB). Skips the test gracefully
    /// if the fixture is not available (e.g. in a fresh checkout that hasn't
    /// pulled it yet).
    fn test_fixture_path() -> Option<std::path::PathBuf> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    /// Read all Vorbis comment entries (FIELD=value strings) from a FLAC file.
    /// Used by tests to verify non-destructive write semantics.
    fn read_all_comments(path: &Path) -> Result<Vec<String>, String> {
        use libflac_sys::*;
        use std::ffi::CString;

        let path_cstr = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let mut out = Vec::new();
        unsafe {
            let chain = FLAC__metadata_chain_new();
            assert!(!chain.is_null());
            assert_ne!(FLAC__metadata_chain_read(chain, path_cstr.as_ptr()), 0);
            let iter = FLAC__metadata_iterator_new();
            FLAC__metadata_iterator_init(iter, chain);
            loop {
                let block = FLAC__metadata_iterator_get_block(iter);
                if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_VORBIS_COMMENT {
                    let vc = &(*block).data.vorbis_comment;
                    for i in 0..vc.num_comments as isize {
                        let entry = *vc.comments.offset(i);
                        let bytes = std::slice::from_raw_parts(entry.entry, entry.length as usize);
                        out.push(String::from_utf8_lossy(bytes).into_owned());
                    }
                    break;
                }
                if FLAC__metadata_iterator_next(iter) == 0 {
                    break;
                }
            }
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
        }
        Ok(out)
    }

    /// Read the Vorbis vendor string. Used to verify it isn't mutated by writes.
    fn read_vendor_string(path: &Path) -> Result<String, String> {
        use libflac_sys::*;
        use std::ffi::CString;

        let path_cstr = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let mut out = String::new();
        unsafe {
            let chain = FLAC__metadata_chain_new();
            assert_ne!(FLAC__metadata_chain_read(chain, path_cstr.as_ptr()), 0);
            let iter = FLAC__metadata_iterator_new();
            FLAC__metadata_iterator_init(iter, chain);
            loop {
                let block = FLAC__metadata_iterator_get_block(iter);
                if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_VORBIS_COMMENT {
                    let vc = &(*block).data.vorbis_comment;
                    let bytes = std::slice::from_raw_parts(
                        vc.vendor_string.entry,
                        vc.vendor_string.length as usize,
                    );
                    out = String::from_utf8_lossy(bytes).into_owned();
                    break;
                }
                if FLAC__metadata_iterator_next(iter) == 0 {
                    break;
                }
            }
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
        }
        Ok(out)
    }

    /// Write a custom set of Vorbis comments (replacing whatever is there) so
    /// each test starts from a known baseline.
    fn set_comments(path: &Path, comments: &[(&str, &str)]) -> Result<(), String> {
        use libflac_sys::*;
        use std::ffi::CString;

        let path_cstr = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            let chain = FLAC__metadata_chain_new();
            assert_ne!(FLAC__metadata_chain_read(chain, path_cstr.as_ptr()), 0);
            let iter = FLAC__metadata_iterator_new();
            FLAC__metadata_iterator_init(iter, chain);
            let mut vc_block: *mut FLAC__StreamMetadata = std::ptr::null_mut();
            loop {
                let block = FLAC__metadata_iterator_get_block(iter);
                if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_VORBIS_COMMENT {
                    vc_block = block;
                    break;
                }
                if FLAC__metadata_iterator_next(iter) == 0 {
                    break;
                }
            }
            assert!(!vc_block.is_null(), "fixture must have a vorbis comment block");
            // Clear existing comments
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
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_sort_padding(chain);
            assert_ne!(FLAC__metadata_chain_write(chain, 1, 0), 0);
            FLAC__metadata_chain_delete(chain);
        }
        Ok(())
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

        // Establish a known baseline of Vorbis comments.
        let baseline: &[(&str, &str)] = &[
            ("ARTIST", "foo"),
            ("TITLE", "bar"),
            ("ALBUM", "baz"),
        ];
        set_comments(&copy, baseline).expect("seed baseline");
        let vendor_before = read_vendor_string(&copy).unwrap();

        // No marker yet.
        assert!(!read_crunched_marker(&copy).unwrap());

        // First write: appends the marker.
        write_crunched_marker(&copy).await.expect("first write");

        let after_first = read_all_comments(&copy).unwrap();
        assert_eq!(
            after_first.len(),
            4,
            "expected 3 baseline + 1 marker, got: {after_first:?}"
        );
        assert!(after_first[0].starts_with("ARTIST=foo"));
        assert!(after_first[1].starts_with("TITLE=bar"));
        assert!(after_first[2].starts_with("ALBUM=baz"));
        assert!(after_first[3].starts_with("FLACCRUNCH_INFO="));
        assert!(after_first[3].contains("\"crunched\":true"));
        assert!(read_crunched_marker(&copy).unwrap());

        let vendor_after_first = read_vendor_string(&copy).unwrap();
        assert_eq!(
            vendor_before, vendor_after_first,
            "vendor string must not change"
        );

        // Brief sleep so crunchedAt timestamp differs.
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Second write: updates the existing marker in place, no duplicate.
        write_crunched_marker(&copy).await.expect("second write");

        let after_second = read_all_comments(&copy).unwrap();
        assert_eq!(
            after_second.len(),
            4,
            "re-crunch must not duplicate the marker; got: {after_second:?}"
        );
        // Original three preserved at original indices with original values.
        assert_eq!(after_second[0], "ARTIST=foo");
        assert_eq!(after_second[1], "TITLE=bar");
        assert_eq!(after_second[2], "ALBUM=baz");
        // Marker still at index 3, refreshed.
        assert!(after_second[3].starts_with("FLACCRUNCH_INFO="));
        assert_ne!(
            after_first[3], after_second[3],
            "crunchedAt timestamp should refresh on re-crunch"
        );

        let vendor_after_second = read_vendor_string(&copy).unwrap();
        assert_eq!(
            vendor_before, vendor_after_second,
            "vendor string must not change across multiple writes"
        );
    }

    #[tokio::test]
    async fn test_read_marker_returns_false_when_absent() {
        let Some(src) = test_fixture_path() else {
            eprintln!("un-optimized.flac fixture missing — skipping");
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let copy = temp.path().join("no_marker.flac");
        std::fs::copy(&src, &copy).unwrap();
        // Reset to a baseline with no FLACCRUNCH_INFO entry.
        set_comments(&copy, &[("ARTIST", "foo")]).unwrap();
        assert!(!read_crunched_marker(&copy).unwrap());
    }
}
