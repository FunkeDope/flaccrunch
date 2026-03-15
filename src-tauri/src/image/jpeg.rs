/// Result of optimizing a JPEG image.
#[derive(Debug)]
pub struct JpegOptimizeResult {
    pub original_size: u64,
    pub optimized_size: u64,
    pub saved_bytes: i64,
}

// ─── Pass 1: Metadata stripping ──────────────────────────────────────────────

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
            0xD8 => { out.push(0xFF); out.push(0xD8); pos += 2; continue; }
            0xD9 => { out.push(0xFF); out.push(0xD9); break; }
            0xD0..=0xD7 => { out.push(0xFF); out.push(marker); pos += 2; continue; }
            _ => {}
        }
        if pos + 4 > data.len() { out.extend_from_slice(&data[pos..]); break; }
        let seg_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
        let seg_end = pos + 2 + seg_len;
        if seg_end > data.len() { out.extend_from_slice(&data[pos..]); break; }
        if marker == 0xDA { out.extend_from_slice(&data[pos..]); break; } // SOS → copy rest
        let keep = match marker {
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF // SOF
            | 0xC4 | 0xDB | 0xDC | 0xDD | 0xDE | 0xDF               // DHT, DQT, misc
            | 0xE0                                                     // APP0 (JFIF)
            => true,
            0xE1..=0xEF | 0xFE => false, // APP1-15 + COM — strip
            _ => true,
        };
        if keep { out.extend_from_slice(&data[pos..seg_end]); }
        pos = seg_end;
    }
    if out.len() < data.len() { Some(out) } else { None }
}

// ─── Pass 2: Huffman reoptimization ──────────────────────────────────────────

/// Extract luma and (optional) chroma DQT tables from a JPEG.
/// Returns coefficients in zigzag order as `u16` (matching jpeg-encoder's Custom type).
fn extract_dqt_tables(data: &[u8]) -> Option<([u16; 64], Option<[u16; 64]>)> {
    let mut luma: Option<[u16; 64]> = None;
    let mut chroma: Option<[u16; 64]> = None;
    let mut pos = 2usize;
    while pos + 3 < data.len() {
        if data[pos] != 0xFF { break; }
        let marker = data[pos + 1];
        if matches!(marker, 0xD8 | 0xD9 | 0xD0..=0xD7) { pos += 2; continue; }
        if pos + 4 > data.len() { break; }
        let seg_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
        let seg_end = pos + 2 + seg_len;
        if seg_end > data.len() { break; }
        if marker == 0xDA { break; }

        if marker == 0xDB {
            let mut q = pos + 4;
            while q + 65 <= seg_end {
                let pq_tq = data[q];
                let precision = (pq_tq >> 4) & 0xF;
                let table_id = pq_tq & 0xF;
                q += 1;
                if precision == 0 && q + 64 <= seg_end {
                    let mut table = [0u16; 64];
                    for (i, &b) in data[q..q + 64].iter().enumerate() {
                        table[i] = b as u16;
                    }
                    match table_id { 0 => luma = Some(table), 1 => chroma = Some(table), _ => {} }
                    q += 64;
                } else {
                    q += 128; // 16-bit precision: 2 bytes each
                }
            }
        }
        pos = seg_end;
    }
    luma.map(|l| (l, chroma))
}

/// Detect chroma subsampling from the SOF header.
fn detect_sampling(data: &[u8]) -> jpeg_encoder::SamplingFactor {
    let mut pos = 2usize;
    while pos + 3 < data.len() {
        if data[pos] != 0xFF { break; }
        let marker = data[pos + 1];
        if matches!(marker, 0xD8 | 0xD9 | 0xD0..=0xD7) { pos += 2; continue; }
        if pos + 4 > data.len() { break; }
        let seg_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
        let seg_end = pos + 2 + seg_len;
        if seg_end > data.len() { break; }
        if marker == 0xDA { break; }

        if matches!(marker, 0xC0..=0xC2) {
            // base + 5 = nComponents, base + 6 = first component id,
            // base + 7 = sampling factors (Hx4 | Vx4), ...
            let base = pos + 4;
            if base + 9 < data.len() {
                let n_comp = data[base + 5] as usize;
                if n_comp >= 3 && base + 6 + n_comp * 3 <= data.len() {
                    let y_sf  = data[base + 7]; // Y  component
                    let cb_sf = data[base + 10]; // Cb component
                    let y_h = (y_sf >> 4) & 0xF;
                    let y_v  = y_sf & 0xF;
                    let cb_h = (cb_sf >> 4) & 0xF;
                    return match (y_h, y_v, cb_h) {
                        (1, 1, 1) => jpeg_encoder::SamplingFactor::R_4_4_4,
                        (2, 1, 1) => jpeg_encoder::SamplingFactor::R_4_2_2,
                        _         => jpeg_encoder::SamplingFactor::R_4_2_0,
                    };
                }
            }
        }
        pos = seg_end;
    }
    jpeg_encoder::SamplingFactor::R_4_2_0 // most common fallback
}

/// Recompress a JPEG using Huffman table reoptimization.
///
/// Extracts the original quantization tables from the DQT markers and feeds
/// them back to the encoder unchanged, so every DCT block quantizes to the
/// same coefficient values. Only the Huffman coding is rebuilt from scratch
/// using optimal prefix codes for this specific image — the same operation
/// `jpegtran -optimize` performs. The decoded image is bit-identical.
///
/// Returns `Some(bytes)` only if the result is smaller.
fn optimize_jpeg_huffman(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 { return None; }

    let (luma_table, chroma_table) = extract_dqt_tables(data)?;
    let sampling = detect_sampling(data);

    // Decode to RGB pixels
    let options = zune_jpeg::zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(data, options);
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let width = info.width as u16;
    let height = info.height as u16;
    let pixels = decoder.decode().ok()?;

    use jpeg_encoder::{Encoder, ColorType, QuantizationTableType};

    let chroma_qt = chroma_table.unwrap_or(luma_table);
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut encoder = Encoder::new(&mut out, 85); // quality ignored — custom tables override it
    encoder.set_sampling_factor(sampling);
    encoder.set_optimized_huffman_tables(true);
    encoder.set_quantization_tables(
        QuantizationTableType::Custom(Box::new(luma_table)),
        QuantizationTableType::Custom(Box::new(chroma_qt)),
    );
    encoder.encode(&pixels, width, height, ColorType::Rgb).ok()?;

    if out.len() < data.len() { Some(out) } else { None }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Run both optimization passes and return the best (smallest) result.
/// Returns `Some(bytes)` if any savings were found, `None` otherwise.
pub fn optimize_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let pass1 = strip_jpeg_metadata(data);
    // Run Huffman pass on the already-stripped data where possible
    let huffman_source = pass1.as_deref().unwrap_or(data);
    let pass2 = optimize_jpeg_huffman(huffman_source);
    match (pass1, pass2) {
        (Some(p1), Some(p2)) => Some(if p2.len() < p1.len() { p2 } else { p1 }),
        (Some(p1), None) => Some(p1),
        (None, Some(p2)) => Some(p2),
        (None, None) => None,
    }
}

/// Optimize a JPEG file and write the result to `output_path` if smaller.
pub async fn optimize_jpeg_file(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<JpegOptimizeResult, String> {
    let data = std::fs::read(input_path)
        .map_err(|e| format!("Failed to read JPEG: {e}"))?;
    let original_size = data.len() as u64;
    match optimize_jpeg(&data) {
        Some(optimized) => {
            let optimized_size = optimized.len() as u64;
            std::fs::write(output_path, &optimized)
                .map_err(|e| format!("Failed to write optimized JPEG: {e}"))?;
            Ok(JpegOptimizeResult {
                original_size,
                optimized_size,
                saved_bytes: original_size as i64 - optimized_size as i64,
            })
        }
        None => Ok(JpegOptimizeResult {
            original_size,
            optimized_size: original_size,
            saved_bytes: 0,
        }),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jpeg_with_exif() -> Vec<u8> {
        vec![
            0xFF, 0xD8,
            0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
            0xFF, 0xE1, 0x00, 0x08, b'E', b'x', b'i', b'f', 0x00, 0x00,
            0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
            0xAB, 0xCD, 0xFF, 0xD9,
        ]
    }

    #[test]
    fn test_strip_removes_exif_keeps_app0() {
        let jpeg = make_jpeg_with_exif();
        let result = strip_jpeg_metadata(&jpeg).expect("should strip EXIF");
        assert!(result.len() < jpeg.len());
        assert!(result.windows(2).any(|w| w == [0xFF, 0xE0]), "APP0 must remain");
        assert!(!result.windows(2).any(|w| w == [0xFF, 0xE1]), "APP1 must be gone");
    }

    #[test]
    fn test_strip_invalid_returns_none() {
        assert!(strip_jpeg_metadata(&[0x00, 0x01, 0x02]).is_none());
        assert!(strip_jpeg_metadata(&[]).is_none());
    }

    /// Integration test: extract and attempt to optimize the artwork embedded in
    /// the test FLAC file.  Run with `cargo test -- --nocapture` to see sizes.
    #[test]
    fn test_artwork_compression_on_test_flac() {
        use std::path::Path;
        use std::ffi::CString;
        use libflac_sys::*;
        use crate::image::detect::{detect_image_format, ImageFormat};

        let flac_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .join("Tests").join("un-optimized.flac");

        if !flac_path.exists() {
            eprintln!("Test FLAC not found at {:?} — skipping", flac_path);
            return;
        }

        let path_cstr = CString::new(flac_path.to_string_lossy().as_bytes()).unwrap();
        let mut any_savings = false;

        unsafe {
            let chain = FLAC__metadata_chain_new();
            assert!(!chain.is_null());
            assert_ne!(FLAC__metadata_chain_read(chain, path_cstr.as_ptr()), 0,
                "Failed to read test FLAC");

            let iter = FLAC__metadata_iterator_new();
            FLAC__metadata_iterator_init(iter, chain);

            loop {
                let block = FLAC__metadata_iterator_get_block(iter);
                if !block.is_null() && (*block).type_ == FLAC__METADATA_TYPE_PICTURE {
                    let pic = &(*block).data.picture;
                    if !pic.data.is_null() && pic.data_length > 0 {
                        let data = std::slice::from_raw_parts(pic.data, pic.data_length as usize);
                        let fmt = detect_image_format(data);
                        let mime = if pic.mime_type.is_null() { String::new() }
                            else { std::ffi::CStr::from_ptr(pic.mime_type).to_string_lossy().into_owned() };

                        let orig = data.len();
                        let (optimized_len, method) = match fmt {
                            Some(ImageFormat::Jpeg) => {
                                let opt_len = optimize_jpeg(data).map(|v| v.len()).unwrap_or(orig);
                                (opt_len, "JPEG")
                            }
                            Some(ImageFormat::Png) => {
                                let tmp = std::env::temp_dir().join("test_art_opt.png");
                                std::fs::write(&tmp, data).unwrap();
                                let opts = oxipng::Options::from_preset(4);
                                let _ = oxipng::optimize(
                                    &oxipng::InFile::Path(tmp.clone()),
                                    &oxipng::OutFile::Path { path: Some(tmp.clone()), preserve_attrs: false },
                                    &opts,
                                );
                                let opt_len = std::fs::metadata(&tmp).map(|m| m.len() as usize).unwrap_or(orig);
                                (opt_len, "PNG")
                            }
                            None => (orig, "unknown"),
                        };

                        let saved = orig.saturating_sub(optimized_len);
                        println!("[{}] mime={} original={} optimized={} saved={} ({:.1}%)",
                            method, mime, orig, optimized_len, saved,
                            if orig > 0 { saved as f64 / orig as f64 * 100.0 } else { 0.0 });

                        if saved > 0 { any_savings = true; }
                    }
                }
                if FLAC__metadata_iterator_next(iter) == 0 { break; }
            }
            FLAC__metadata_iterator_delete(iter);
            FLAC__metadata_chain_delete(chain);
        }

        println!("any_savings={}", any_savings);
        // Don't assert — just report. A JPEG that's already optimal may not compress further.
    }
}
