//! Lossless JPEG optimization for embedded artwork.
//!
//! Two passes, both bit-exact for the decoded image:
//!
//! 1. **Segment stripping** — drop APP1–APP15 and COM segments (EXIF, XMP,
//!    ICC, comments). Pure byte surgery; the entropy-coded data is untouched.
//! 2. **Coefficient-level Huffman re-optimization** — the same transform as
//!    `jpegtran -optimize`: libjpeg (mozjpeg) reads the quantized DCT
//!    coefficients, builds optimal Huffman tables for this image and writes
//!    the *same* coefficients back. Both a baseline and a progressive scan
//!    layout are tried and the smaller one kept. No pixels are ever decoded
//!    or re-quantized.
//!
//! As a belt-and-braces check, the result is only accepted if it decodes
//! (via zune-jpeg) to exactly the same pixels as the input.

use mozjpeg_sys::*;
use std::ffi::CStr;
use std::os::raw::{c_int, c_ulong};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

// ─── Pass 1: Segment stripping ───────────────────────────────────────────────

/// Losslessly strip non-essential APP markers from a JPEG.
///
/// Removes APP1 (EXIF/XMP), APP2 (ICC profile), APP3–APP15, COM.
/// Keeps APP0 (JFIF) and all image-critical markers (SOF, DHT, DQT, SOS, RST, EOI).
/// Returns `Some(bytes)` only if the stripped version is smaller.
fn strip_jpeg_metadata(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut pos = 0usize;

    while pos + 1 < data.len() {
        if data[pos] != 0xFF {
            return None;
        }
        let marker = data[pos + 1];
        match marker {
            0xD8 => {
                out.push(0xFF);
                out.push(0xD8);
                pos += 2;
                continue;
            }
            0xD9 => {
                out.push(0xFF);
                out.push(0xD9);
                break;
            }
            0xD0..=0xD7 => {
                out.push(0xFF);
                out.push(marker);
                pos += 2;
                continue;
            }
            _ => {}
        }
        if pos + 4 > data.len() {
            out.extend_from_slice(&data[pos..]);
            break;
        }
        let seg_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
        let seg_end = pos + 2 + seg_len;
        if seg_end > data.len() {
            out.extend_from_slice(&data[pos..]);
            break;
        }
        if marker == 0xDA {
            out.extend_from_slice(&data[pos..]);
            break;
        } // SOS → copy rest
        let keep = match marker {
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF // SOF
            | 0xC4 | 0xDB | 0xDC | 0xDD | 0xDE | 0xDF               // DHT, DQT, misc
            | 0xE0                                                     // APP0 (JFIF)
            => true,
            0xE1..=0xEF | 0xFE => false, // APP1-15 + COM — strip
            _ => true,
        };
        if keep {
            out.extend_from_slice(&data[pos..seg_end]);
        }
        pos = seg_end;
    }
    if out.len() < data.len() {
        Some(out)
    } else {
        None
    }
}

// ─── Pass 2: Coefficient-level Huffman re-optimization (mozjpeg) ─────────────

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

/// Re-write `data` with optimal Huffman tables, keeping every DCT
/// coefficient as-is. `progressive` selects the scan layout.
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
        jpeg_read_header(src, 1);
        let coefficients = jpeg_read_coefficients(src);
        if coefficients.is_null() {
            return Err("jpeg_read_coefficients returned NULL".to_string());
        }

        jpeg_mem_dest(dst, out_buf, out_size);
        // Copies colour space, sampling factors, quantization tables, JFIF
        // flags… and applies mozjpeg's defaults (progressive scan script,
        // scan optimization). We then dial the scan layout explicitly.
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
fn decode_pixels(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(data, options);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    Some((info.width as usize, info.height as usize, pixels))
}

/// Smallest of the baseline/progressive re-optimizations of `data`, verified
/// to decode identically. `None` if nothing smaller (or verifiable) came out.
fn optimize_jpeg_huffman(data: &[u8]) -> Option<Vec<u8>> {
    let candidates = [transcode(data, false), transcode(data, true)];
    let best = candidates
        .into_iter()
        .filter_map(Result::ok)
        .filter(|c| c.len() < data.len())
        .min_by_key(Vec::len)?;

    // Lossless by construction; verified anyway before touching the file.
    let original = decode_pixels(data)?;
    let rewritten = decode_pixels(&best)?;
    (original == rewritten).then_some(best)
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Optimize a JPEG in memory, losslessly. Returns `Some(bytes)` if the result
/// is smaller than the input, `None` otherwise.
pub fn optimize_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let stripped = strip_jpeg_metadata(data);
    let source = stripped.as_deref().unwrap_or(data);
    match (stripped.as_ref(), optimize_jpeg_huffman(source)) {
        (_, Some(reoptimized)) => Some(reoptimized),
        (Some(_), None) => stripped,
        (None, None) => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jpeg_with_exif() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xE1, 0x00, 0x08, b'E', b'x', b'i', b'f',
            0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xAB, 0xCD,
            0xFF, 0xD9,
        ]
    }

    /// A real 96×64 JPEG encoded with libjpeg's *default* (non-optimized)
    /// Huffman tables, so there is something for pass 2 to gain.
    fn make_unoptimized_jpeg() -> Vec<u8> {
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
        enc.encode(&pixels, w, h, ColorType::Rgb).expect("encode");
        out
    }

    #[test]
    fn test_strip_removes_exif_keeps_app0() {
        let jpeg = make_jpeg_with_exif();
        let result = strip_jpeg_metadata(&jpeg).expect("should strip EXIF");
        assert!(result.len() < jpeg.len());
        assert!(
            result.windows(2).any(|w| w == [0xFF, 0xE0]),
            "APP0 must remain"
        );
        assert!(
            !result.windows(2).any(|w| w == [0xFF, 0xE1]),
            "APP1 must be gone"
        );
    }

    #[test]
    fn test_strip_invalid_returns_none() {
        assert!(strip_jpeg_metadata(&[0x00, 0x01, 0x02]).is_none());
        assert!(strip_jpeg_metadata(&[]).is_none());
    }

    #[test]
    fn test_transcode_rejects_garbage_without_crashing() {
        assert!(transcode(b"definitely not a jpeg", false).is_err());
        assert!(
            transcode(&make_jpeg_with_exif(), true).is_err(),
            "truncated scan data"
        );
        // The strip-only fallback still applies to such inputs.
        let jpeg = make_jpeg_with_exif();
        let out = optimize_jpeg(&jpeg).expect("stripping still saves bytes");
        assert_eq!(out, strip_jpeg_metadata(&jpeg).unwrap());
    }

    #[test]
    fn test_huffman_reoptimization_is_smaller_and_pixel_identical() {
        let jpeg = make_unoptimized_jpeg();
        let out = optimize_jpeg(&jpeg).expect("non-optimized tables leave room to gain");
        assert!(out.len() < jpeg.len(), "{} vs {}", out.len(), jpeg.len());
        assert_eq!(decode_pixels(&jpeg).unwrap(), decode_pixels(&out).unwrap());

        // Idempotent: nothing left to gain on an already optimal file.
        assert!(optimize_jpeg(&out).is_none());
    }

    #[test]
    fn test_both_scan_layouts_are_lossless() {
        let jpeg = make_unoptimized_jpeg();
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
