//! Lossless PNG optimization via oxipng.
//!
//! Only the compression is touched: oxipng re-filters and re-deflates the
//! image data at preset 4. Colour-type / bit-depth / palette reductions and
//! alpha "optimization" are disabled — they are visually lossless but change
//! the stored pixel values, and the product promise is that the decoded data
//! is identical. All ancillary chunks (text, ICC, gamma…) are kept.

/// Return a losslessly re-compressed encoding of `data`. The result may be
/// larger than the input for already-optimal files; callers compare lengths.
pub fn optimize_png(data: &[u8]) -> Result<Vec<u8>, String> {
    let options = oxipng::Options {
        bit_depth_reduction: false,
        color_type_reduction: false,
        palette_reduction: false,
        grayscale_reduction: false,
        optimize_alpha: false,
        strip: oxipng::StripChunks::None,
        ..oxipng::Options::from_preset(4)
    };
    oxipng::optimize_from_memory(data, &options)
        .map_err(|e| format!("oxipng optimization failed: {e}"))
}

/// Decoded PNG: header fields plus the raw (unfiltered, untransformed)
/// image data. Two PNGs with equal `DecodedPng` hold identical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPng {
    pub width: u32,
    pub height: u32,
    pub color_type: png::ColorType,
    pub bit_depth: png::BitDepth,
    pub data: Vec<u8>,
}

/// Decode without any transformation. `None` if the data isn't a valid PNG.
pub(crate) fn decode_png(data: &[u8]) -> Option<DecodedPng> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some(DecodedPng {
        width: info.width,
        height: info.height,
        color_type: info.color_type,
        bit_depth: info.bit_depth,
        data: buf,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            let mut val = (crc ^ byte as u32) & 0xFF;
            for _ in 0..8 {
                val = if val & 1 != 0 {
                    (val >> 1) ^ 0xEDB8_8320
                } else {
                    val >> 1
                };
            }
            crc = (crc >> 8) ^ val;
        }
        !crc
    }

    fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut c = tag.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
        out
    }

    /// Stored (uncompressed) zlib stream for `raw`.
    fn zlib_stored(raw: &[u8]) -> Vec<u8> {
        let mut idat = vec![0x78, 0x01];
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;
        for &b in raw {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        let blocks: Vec<&[u8]> = if raw.is_empty() {
            vec![&[][..]]
        } else {
            raw.chunks(65535).collect()
        };
        for (i, block) in blocks.iter().enumerate() {
            idat.push(if i + 1 == blocks.len() { 1 } else { 0 });
            idat.extend_from_slice(&(block.len() as u16).to_le_bytes());
            idat.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            idat.extend_from_slice(block);
        }
        idat.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
        idat
    }

    /// RGB PNG of the given size with stored (uncompressed) IDAT and a tEXt
    /// chunk, so recompression has plenty to gain and metadata to keep.
    pub(crate) fn stored_rgb_png(w: u32, h: u32) -> Vec<u8> {
        let row: Vec<u8> = std::iter::once(0u8)
            .chain((0..w).flat_map(|x| [(x * 7) as u8, 20, (x * 3) as u8]))
            .collect();
        let raw: Vec<u8> = row.repeat(h as usize);
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(b"tEXt", b"Comment\0keep me"));
        png.extend_from_slice(&chunk(b"IDAT", &zlib_stored(&raw)));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    /// Build a minimal but valid 1×1 RGB PNG in memory.
    pub(crate) fn minimal_1x1_rgb_png() -> Vec<u8> {
        stored_rgb_png(1, 1)
    }

    #[test]
    fn test_optimize_png_on_valid_png_succeeds() {
        let png = minimal_1x1_rgb_png();
        let out = optimize_png(&png).expect("optimize_png should succeed");
        assert_eq!(&out[..8], &png[..8], "output is still a PNG");
        assert_eq!(decode_png(&png), decode_png(&out));
    }

    #[test]
    fn test_optimize_png_on_invalid_data_returns_err() {
        assert!(optimize_png(b"not a png at all").is_err());
        assert!(decode_png(b"not a png at all").is_none());
    }

    #[test]
    fn test_optimize_png_shrinks_bloated_png_and_keeps_everything() {
        let png = stored_rgb_png(64, 64);
        let out = optimize_png(&png).unwrap();
        assert!(out.len() < png.len() / 4, "{} vs {}", out.len(), png.len());
        // Same header, same raw pixel bytes, text chunk kept.
        assert_eq!(decode_png(&png).unwrap(), decode_png(&out).unwrap());
        assert!(
            out.windows(4).any(|w| w == b"tEXt"),
            "ancillary chunks must be preserved"
        );
    }
}
