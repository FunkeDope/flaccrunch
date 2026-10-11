//! Lossless JPEG optimization for embedded artwork.
//!
//! The same transform as `jpegtran -optimize`: libjpeg (mozjpeg) reads the
//! quantized DCT coefficients, builds optimal Huffman tables for this image
//! and writes the *same* coefficients back. Both a baseline and a progressive
//! scan layout are tried and the smaller one kept. No pixels are ever decoded
//! or re-quantized, and every APP/COM segment (EXIF, XMP, ICC, comments…) is
//! carried over verbatim, so the only bytes that change are the entropy-coded
//! data and the Huffman tables.
//!
//! As a belt-and-braces check, the result is only accepted if it decodes
//! (via zune-jpeg) to exactly the same pixels as the input.

use mozjpeg_sys::*;
use std::ffi::CStr;
use std::os::raw::{c_int, c_ulong};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

const JPEG_APP0: c_int = 0xE0;
const JPEG_COM: c_int = 0xFE;

/// Payload carried through the unwind when libjpeg reports an error.
struct JpegError(String);

/// libjpeg's `error_exit` must not return. mozjpeg-sys compiles the C code
/// with unwinding support and declares the callback `extern "C-unwind"`, so
/// unwinding out of it is the supported way to abort (the `mozjpeg` crate
/// does the same). `catch_unwind` in [`transcode`] turns it into an `Err`.
unsafe extern "C-unwind" fn error_exit(cinfo: &mut jpeg_common_struct) {
    // The binding declares the buffer as `&[u8; 80]` although C writes into
    // it; UnsafeCell makes that interior mutation well-defined on our side.
    let buf = std::cell::UnsafeCell::new([0u8; 80]);
    if let Some(format) = (*cinfo.err).format_message {
        format(cinfo, &*buf.get());
    }
    let msg = CStr::from_bytes_until_nul(&*buf.get())
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "libjpeg error".to_string());
    resume_unwind(Box::new(JpegError(msg)));
}

/// Swallow warnings (corrupt-data notices etc.); we never want C code
/// printing to stderr from inside a worker.
unsafe extern "C-unwind" fn emit_message(_: &mut jpeg_common_struct, _: c_int) {}
unsafe extern "C-unwind" fn output_message(_: &mut jpeg_common_struct) {}

fn quiet_error_mgr() -> jpeg_error_mgr {
    let mut err: jpeg_error_mgr = unsafe { std::mem::zeroed() };
    unsafe { jpeg_std_error(&mut err) };
    err.error_exit = Some(error_exit);
    err.emit_message = Some(emit_message);
    err.output_message = Some(output_message);
    err
}

/// Releases every libjpeg object on all exit paths, including unwinding out
/// of a libjpeg error (where `jpeg_destroy_*` is exactly what libjpeg expects
/// the caller to do).
struct Codecs {
    src: jpeg_decompress_struct,
    dst: jpeg_compress_struct,
    out_buf: *mut u8,
    out_size: c_ulong,
}

impl Drop for Codecs {
    fn drop(&mut self) {
        unsafe {
            jpeg_destroy_compress(&mut self.dst);
            jpeg_destroy_decompress(&mut self.src);
            if !self.out_buf.is_null() {
                // jpeg_mem_dest allocates with malloc and leaves ownership to us.
                libc::free(self.out_buf as *mut libc::c_void);
            }
        }
    }
}

/// Copy every saved APP/COM segment from the decoder to the encoder, except
/// the ones libjpeg regenerates itself (JFIF APP0 and Adobe APP14), exactly
/// like jpegtran's `jcopy_markers_execute`.
unsafe fn copy_markers(src: &jpeg_decompress_struct, dst: &mut jpeg_compress_struct) {
    let mut marker = src.marker_list;
    while !marker.is_null() {
        let m = &*marker;
        let data = if m.data.is_null() || m.data_length == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(m.data, m.data_length as usize)
        };
        let is_jfif = m.marker as c_int == JPEG_APP0
            && dst.write_JFIF_header != 0
            && data.len() >= 5
            && &data[..5] == b"JFIF\0";
        let is_adobe = m.marker as c_int == JPEG_APP0 + 14
            && dst.write_Adobe_marker != 0
            && data.len() >= 5
            && &data[..5] == b"Adobe";
        if !is_jfif && !is_adobe {
            jpeg_write_marker(dst, m.marker as c_int, data.as_ptr(), data.len() as c_uint);
        }
        marker = m.next;
    }
}

use std::os::raw::c_uint;

/// Re-write `data` with optimal Huffman tables, keeping every DCT
/// coefficient and every metadata segment as-is. `progressive` selects the
/// scan layout.
fn transcode(data: &[u8], progressive: bool) -> Result<Vec<u8>, String> {
    // The error managers must outlive the codecs that point at them.
    let mut src_err = quiet_error_mgr();
    let mut dst_err = quiet_error_mgr();

    let mut codecs = Codecs {
        src: unsafe { std::mem::zeroed() },
        dst: unsafe { std::mem::zeroed() },
        out_buf: std::ptr::null_mut(),
        out_size: 0,
    };
    codecs.src.common.err = &mut src_err;
    codecs.dst.common.err = &mut dst_err;
    unsafe {
        jpeg_create_decompress(&mut codecs.src);
        jpeg_create_compress(&mut codecs.dst);
    }

    let outcome = catch_unwind(AssertUnwindSafe(|| unsafe {
        let Codecs {
            src,
            dst,
            out_buf,
            out_size,
        } = &mut codecs;

        jpeg_mem_src(src, data.as_ptr(), data.len() as c_ulong);
        // Keep every APPn and COM segment so they can be written back.
        for n in 0..16 {
            jpeg_save_markers(src, JPEG_APP0 + n, 0xFFFF);
        }
        jpeg_save_markers(src, JPEG_COM, 0xFFFF);
        jpeg_read_header(src, 1);
        let coefficients = jpeg_read_coefficients(src);
        if coefficients.is_null() {
            return Err("jpeg_read_coefficients returned NULL".to_string());
        }

        jpeg_mem_dest(dst, out_buf, out_size);
        // Copies colour space, sampling factors, quantization tables, JFIF
        // flags/density… and applies mozjpeg's defaults (progressive scan
        // script, scan optimization). We then dial the scan layout explicitly.
        jpeg_copy_critical_parameters(src, dst);
        dst.optimize_coding = 1;
        if progressive {
            jpeg_simple_progression(dst);
            jpeg_c_set_bool_param(dst, J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS, 1);
        } else {
            dst.scan_info = std::ptr::null();
            dst.num_scans = 0;
            jpeg_c_set_bool_param(dst, J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS, 0);
        }

        jpeg_write_coefficients(dst, coefficients);
        copy_markers(src, dst);
        jpeg_finish_compress(dst);
        jpeg_finish_decompress(src);

        if out_buf.is_null() || *out_size == 0 {
            return Err("libjpeg produced no output".to_string());
        }
        Ok(std::slice::from_raw_parts(*out_buf, *out_size as usize).to_vec())
    }));

    match outcome {
        Ok(result) => result,
        Err(payload) => Err(payload
            .downcast::<JpegError>()
            .map(|e| e.0)
            .unwrap_or_else(|_| "libjpeg error".to_string())),
    }
}

/// Decode to RGB for the equality check. `None` if zune-jpeg can't decode
/// (e.g. CMYK, 12-bit) — in which case we refuse to replace the picture.
pub(crate) fn decode_pixels(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(data, options);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    Some((info.width as usize, info.height as usize, pixels))
}

/// Every APPn/COM segment of a JPEG, in order, as (marker, payload).
pub(crate) fn metadata_segments(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return out;
    }
    let mut pos = 2usize;
    while pos + 4 <= data.len() && data[pos] == 0xFF {
        let marker = data[pos + 1];
        if matches!(marker, 0xD0..=0xD9) {
            pos += 2;
            continue;
        }
        let seg_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
        let seg_end = pos + 2 + seg_len;
        if seg_end > data.len() || marker == 0xDA {
            break;
        }
        if matches!(marker, 0xE0..=0xEF | 0xFE) {
            out.push((marker, data[pos + 4..seg_end].to_vec()));
        }
        pos = seg_end;
    }
    out
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Optimize a JPEG in memory, losslessly. Returns `Some(bytes)` if a smaller,
/// pixel-identical, metadata-identical encoding was found, `None` otherwise.
pub fn optimize_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let candidates = [transcode(data, false), transcode(data, true)];
    let best = candidates
        .into_iter()
        .filter_map(Result::ok)
        .filter(|c| c.len() < data.len())
        .min_by_key(Vec::len)?;

    // Lossless by construction; verified anyway before touching the file.
    if decode_pixels(data)? != decode_pixels(&best)? {
        return None;
    }
    // JFIF APP0 is regenerated by libjpeg (same version/density, so normally
    // identical); everything else must match byte for byte.
    let non_jfif = |segs: Vec<(u8, Vec<u8>)>| -> Vec<(u8, Vec<u8>)> {
        segs.into_iter()
            .filter(|(m, d)| !(*m == 0xE0 && d.starts_with(b"JFIF\0")))
            .collect()
    };
    if non_jfif(metadata_segments(data)) != non_jfif(metadata_segments(&best)) {
        return None;
    }
    Some(best)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A real 96×64 JPEG encoded with libjpeg's *default* (non-optimized)
    /// Huffman tables, carrying an APP1 "Exif" blob, an APP2 ICC-ish blob and
    /// a COM segment, so there is something for pass 2 to gain and markers to
    /// preserve.
    pub(crate) fn make_unoptimized_jpeg_with_markers() -> Vec<u8> {
        use jpeg_encoder::{ColorType, Encoder};
        let (w, h) = (96u16, 64u16);
        let mut pixels = Vec::with_capacity(w as usize * h as usize * 3);
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&[
                    (x * 2) as u8,
                    (y * 3) as u8,
                    ((x as u32 * y as u32) % 251) as u8,
                ]);
            }
        }
        let mut out = Vec::new();
        let mut enc = Encoder::new(&mut out, 80);
        enc.set_optimized_huffman_tables(false);
        enc.add_app_segment(1, b"Exif\0\0fake exif payload 1234")
            .unwrap();
        enc.add_app_segment(2, b"ICC_PROFILE\0\x01\x01not really a profile")
            .unwrap();
        enc.add_app_segment(13, b"Photoshop 3.0\0some 8BIM")
            .unwrap();
        enc.encode(&pixels, w, h, ColorType::Rgb).expect("encode");
        out
    }

    #[test]
    fn test_transcode_rejects_garbage_without_crashing() {
        assert!(transcode(b"definitely not a jpeg", false).is_err());
        assert!(optimize_jpeg(b"\xFF\xD8\xFF\xD9").is_none());
    }

    #[test]
    fn test_huffman_reoptimization_is_smaller_and_pixel_identical() {
        let jpeg = make_unoptimized_jpeg_with_markers();
        let out = optimize_jpeg(&jpeg).expect("non-optimized tables leave room to gain");
        assert!(out.len() < jpeg.len(), "{} vs {}", out.len(), jpeg.len());
        assert_eq!(decode_pixels(&jpeg).unwrap(), decode_pixels(&out).unwrap());

        // Idempotent: nothing left to gain on an already optimal file.
        assert!(optimize_jpeg(&out).is_none());
    }

    #[test]
    fn test_every_metadata_segment_survives() {
        let jpeg = make_unoptimized_jpeg_with_markers();
        let before = metadata_segments(&jpeg);
        let markers: Vec<u8> = before.iter().map(|(m, _)| *m).collect();
        for m in [0xE1u8, 0xE2, 0xED] {
            assert!(
                markers.contains(&m),
                "missing APP{}: {markers:x?}",
                m - 0xE0
            );
        }
        let out = optimize_jpeg(&jpeg).expect("optimizes");
        let after = metadata_segments(&out);
        assert_eq!(
            before, after,
            "all APP/COM segments byte-identical and in order"
        );
    }

    #[test]
    fn test_both_scan_layouts_are_lossless() {
        let jpeg = make_unoptimized_jpeg_with_markers();
        let reference = decode_pixels(&jpeg).unwrap();
        for progressive in [false, true] {
            let out = transcode(&jpeg, progressive).expect("transcode");
            assert_eq!(
                decode_pixels(&out).unwrap(),
                reference,
                "progressive={progressive}"
            );
        }
    }

    /// Fixture artwork: optimization must be lossless there too.
    #[test]
    fn test_fixture_artwork_is_optimized_losslessly() {
        use crate::flac::metadata::MetadataChain;
        use crate::image::detect::{detect_image_format, ImageFormat};
        use libflac_sys::*;
        use std::path::Path;

        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        if !flac_path.exists() {
            eprintln!("Test FLAC not found — skipping");
            return;
        }
        let chain = MetadataChain::read(&flac_path).unwrap();
        let iter = chain.iter().unwrap();
        loop {
            if iter.block_type() == FLAC__METADATA_TYPE_PICTURE {
                let pic = unsafe { &(*iter.block()).data.picture };
                let data =
                    unsafe { std::slice::from_raw_parts(pic.data, pic.data_length as usize) };
                if detect_image_format(data) == Some(ImageFormat::Jpeg) {
                    let orig = data.len();
                    match optimize_jpeg(data) {
                        Some(out) => {
                            assert!(out.len() < orig);
                            assert_eq!(decode_pixels(data).unwrap(), decode_pixels(&out).unwrap());
                            assert_eq!(metadata_segments(data), metadata_segments(&out));
                            assert!(optimize_jpeg(&out).is_none(), "second pass must be a no-op");
                            println!(
                                "[JPEG] original={orig} optimized={} saved={} ({:.1}%)",
                                out.len(),
                                orig - out.len(),
                                (orig - out.len()) as f64 / orig as f64 * 100.0
                            );
                        }
                        None => println!("[JPEG] original={orig} already optimal"),
                    }
                }
            }
            if !iter.next() {
                break;
            }
        }
    }
}
