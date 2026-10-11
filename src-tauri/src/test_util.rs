//! Test helpers: a tiny synthetic FLAC with tags and pictures, generated in
//! milliseconds, so the pipeline can be exercised without re-encoding the
//! 45 MB fixture every time.

#![cfg(test)]

use crate::flac::ffi::path_cstring;
use crate::flac::metadata::MetadataChain;
use libflac_sys::*;
use std::ffi::CString;
use std::path::{Path, PathBuf};

/// A valid 1×1 RGB PNG.
pub fn tiny_png() -> Vec<u8> {
    crate::image::png::tests::minimal_1x1_rgb_png()
}

/// A valid baseline JPEG (96×64, default Huffman tables) carrying an APP1
/// segment and a comment, so marker preservation can be checked.
pub fn tiny_jpeg_with_markers() -> Vec<u8> {
    crate::image::jpeg::tests::make_unoptimized_jpeg_with_markers()
}

/// Write `seconds` of synthetic stereo 16-bit/44.1 kHz audio as a FLAC with a
/// vendor string, four tags, a PNG and a JPEG picture and a PADDING block.
/// Encoded at level 0 so the re-encode has something to gain.
pub fn small_flac(dir: &Path, name: &str, seconds: u32) -> PathBuf {
    let path = dir.join(name);
    let path_c = path_cstring(&path).unwrap();
    let sample_rate = 44_100u32;
    let total = (sample_rate * seconds) as usize;

    unsafe {
        // Metadata blocks attached to the encoder (all legal here).
        let vc = FLAC__metadata_object_new(FLAC__METADATA_TYPE_VORBIS_COMMENT);
        set_vendor(vc, b"test-vendor 1.0");
        for (k, v) in [
            ("ARTIST", "Some Artist"),
            ("ALBUM", "Some Album"),
            ("TITLE", "Track \u{e9}"),
            ("TRACKNUMBER", "7"),
        ] {
            append_comment(vc, k, v);
        }
        let pic_png = picture_block(b"image/png", b"Front", 3, &tiny_png());
        let pic_jpg = picture_block(
            b"image/jpeg",
            "Back \u{263a}".as_bytes(),
            4,
            &tiny_jpeg_with_markers(),
        );
        let app = FLAC__metadata_object_new(FLAC__METADATA_TYPE_APPLICATION);
        (*app).data.application.id = *b"test";
        let payload = b"application payload".to_vec();
        assert_ne!(
            FLAC__metadata_object_application_set_data(
                app,
                payload.as_ptr() as *mut u8,
                payload.len() as u32,
                1
            ),
            0
        );
        let pad = FLAC__metadata_object_new(FLAC__METADATA_TYPE_PADDING);
        (*pad).length = 4096;
        let mut blocks = [vc, pic_png, pic_jpg, app, pad];

        let enc = FLAC__stream_encoder_new();
        FLAC__stream_encoder_set_channels(enc, 2);
        FLAC__stream_encoder_set_bits_per_sample(enc, 16);
        FLAC__stream_encoder_set_sample_rate(enc, sample_rate);
        FLAC__stream_encoder_set_compression_level(enc, 0);
        FLAC__stream_encoder_set_total_samples_estimate(enc, total as u64);
        assert_ne!(
            FLAC__stream_encoder_set_metadata(enc, blocks.as_mut_ptr(), blocks.len() as u32),
            0
        );
        assert_eq!(
            FLAC__stream_encoder_init_file(enc, path_c.as_ptr(), None, std::ptr::null_mut()),
            FLAC__STREAM_ENCODER_INIT_STATUS_OK
        );
        // Two detuned sines plus a little noise so compression is non-trivial.
        let mut buf: Vec<i32> = Vec::with_capacity(4096 * 2);
        let mut seed: u32 = 12345;
        let mut n = 0usize;
        while n < total {
            buf.clear();
            let chunk = 4096.min(total - n);
            for i in 0..chunk {
                let t = (n + i) as f64 / sample_rate as f64;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((seed >> 16) as i32 & 0xff) - 128;
                let l = (12000.0 * (t * 440.0 * std::f64::consts::TAU).sin()) as i32 + noise;
                let r = (9000.0 * (t * 554.37 * std::f64::consts::TAU).sin()) as i32 - noise;
                buf.push(l);
                buf.push(r);
            }
            assert_ne!(
                FLAC__stream_encoder_process_interleaved(enc, buf.as_ptr(), chunk as u32),
                0
            );
            n += chunk;
        }
        assert_ne!(FLAC__stream_encoder_finish(enc), 0);
        FLAC__stream_encoder_delete(enc);
        for b in blocks {
            FLAC__metadata_object_delete(b);
        }
    }
    path
}

/// Append a raw comment entry (any bytes — may be illegal per the Vorbis
/// spec) to the first VORBIS_COMMENT block of `path`, bypassing libFLAC's
/// validating setters. Models what careless taggers write.
pub fn plant_illegal_comment(path: &Path, raw: &[u8]) {
    let chain = MetadataChain::read(path).unwrap();
    let iter = chain.iter().unwrap();
    let vc = iter
        .find(FLAC__METADATA_TYPE_VORBIS_COMMENT)
        .expect("fixture has a VORBIS_COMMENT block");
    unsafe {
        let n = (*vc).data.vorbis_comment.num_comments;
        assert_ne!(
            FLAC__metadata_object_vorbiscomment_resize_comments(vc, n + 1),
            0
        );
        let entry = &mut *(*vc).data.vorbis_comment.comments.add(n as usize);
        if !entry.entry.is_null() {
            libc::free(entry.entry as *mut libc::c_void);
        }
        // libFLAC frees entries with free(), so allocate with malloc.
        let mem = libc::malloc(raw.len() + 1) as *mut u8;
        std::ptr::copy_nonoverlapping(raw.as_ptr(), mem, raw.len());
        *mem.add(raw.len()) = 0;
        entry.entry = mem;
        entry.length = raw.len() as u32;
        // resize_comments already accounted for an empty entry (4-byte length
        // prefix); only the payload bytes are new.
        (*vc).length += raw.len() as u32;
    }
    drop(iter);
    chain.write(true).unwrap();
}

/// Block types of `path` in file order.
pub fn block_types(path: &Path) -> Vec<FLAC__MetadataType> {
    let chain = MetadataChain::read(path).unwrap();
    let iter = chain.iter().unwrap();
    let mut out = Vec::new();
    loop {
        out.push(iter.block_type());
        if !iter.next() {
            break;
        }
    }
    out
}

unsafe fn set_vendor(vc: *mut FLAC__StreamMetadata, vendor: &[u8]) {
    let mut entry: FLAC__StreamMetadata_VorbisComment_Entry = std::mem::zeroed();
    let c = CString::new(vendor).unwrap();
    entry.entry = c.as_ptr() as *mut u8;
    entry.length = vendor.len() as u32;
    assert_ne!(
        FLAC__metadata_object_vorbiscomment_set_vendor_string(vc, entry, 1),
        0
    );
}

unsafe fn append_comment(vc: *mut FLAC__StreamMetadata, name: &str, value: &str) {
    let n = CString::new(name).unwrap();
    let v = CString::new(value).unwrap();
    let mut entry: FLAC__StreamMetadata_VorbisComment_Entry = std::mem::zeroed();
    assert_ne!(
        FLAC__metadata_object_vorbiscomment_entry_from_name_value_pair(
            &mut entry,
            n.as_ptr(),
            v.as_ptr()
        ),
        0
    );
    assert_ne!(
        FLAC__metadata_object_vorbiscomment_append_comment(vc, entry, 1),
        0
    );
    libc::free(entry.entry as *mut libc::c_void);
}

unsafe fn picture_block(
    mime: &[u8],
    description: &[u8],
    kind: u32,
    data: &[u8],
) -> *mut FLAC__StreamMetadata {
    let pic = FLAC__metadata_object_new(FLAC__METADATA_TYPE_PICTURE);
    (*pic).data.picture.type_ = kind;
    let mime_c = CString::new(mime).unwrap();
    let desc_c = CString::new(description).unwrap();
    assert_ne!(
        FLAC__metadata_object_picture_set_mime_type(pic, mime_c.as_ptr() as *mut _, 1),
        0
    );
    assert_ne!(
        FLAC__metadata_object_picture_set_description(pic, desc_c.as_ptr() as *mut u8, 1),
        0
    );
    assert_ne!(
        FLAC__metadata_object_picture_set_data(pic, data.as_ptr() as *mut u8, data.len() as u32, 1),
        0
    );
    pic
}
