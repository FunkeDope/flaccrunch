//! Lossless PNG optimization via oxipng (preset 4, alpha optimization on).

/// Return a losslessly optimized encoding of `data`. The result may be larger
/// than the input for already-optimal files; callers should compare lengths.
pub fn optimize_png(data: &[u8]) -> Result<Vec<u8>, String> {
    let options = oxipng::Options {
        optimize_alpha: true,
        ..oxipng::Options::from_preset(4)
    };
    oxipng::optimize_from_memory(data, &options)
        .map_err(|e| format!("oxipng optimization failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but valid 1×1 RGB PNG in memory.
    pub(crate) fn minimal_1x1_rgb_png() -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc: u32 = 0xFFFF_FFFF;
            for &byte in data {
                let mut val = (crc ^ byte as u32) & 0xFF;
                for _ in 0..8 {
                    if val & 1 != 0 {
                        val = (val >> 1) ^ 0xEDB8_8320;
                    } else {
                        val >>= 1;
                    }
                }
                crc = (crc >> 8) ^ val;
            }
            !crc
        }

        fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(tag);
            out.extend_from_slice(data);
            let mut crc_input = tag.to_vec();
            crc_input.extend_from_slice(data);
            out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
            out
        }

        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));

        // zlib stored block containing: filter byte 0 + RGB white.
        let scanline: [u8; 4] = [0x00, 0xFF, 0xFF, 0xFF];
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;
        for &b in &scanline {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        let mut idat = vec![0x78, 0x01, 0x01];
        idat.extend_from_slice(&(scanline.len() as u16).to_le_bytes());
        idat.extend_from_slice(&(!(scanline.len() as u16)).to_le_bytes());
        idat.extend_from_slice(&scanline);
        idat.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
        png.extend_from_slice(&chunk(b"IDAT", &idat));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn test_optimize_png_on_valid_png_succeeds() {
        let png = minimal_1x1_rgb_png();
        let out = optimize_png(&png).expect("optimize_png should succeed");
        assert!(!out.is_empty());
        assert_eq!(&out[..8], &png[..8], "output is still a PNG");
    }

    #[test]
    fn test_optimize_png_on_invalid_data_returns_err() {
        assert!(optimize_png(b"not a png at all").is_err());
    }

    #[test]
    fn test_optimize_png_shrinks_bloated_png() {
        // A stored (uncompressed) 64×64 RGB image compresses dramatically.
        let w = 64u32;
        let h = 64u32;
        // Each row: filter byte 0 followed by w RGB pixels.
        let row: Vec<u8> = std::iter::once(0u8)
            .chain([10u8, 20, 30].repeat(w as usize))
            .collect();
        let raw: Vec<u8> = row.repeat(h as usize);
        // Encode with stored deflate blocks (max 65535 bytes each).
        let mut idat = vec![0x78, 0x01];
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;
        for &b in &raw {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        for (i, block) in raw.chunks(65535).enumerate() {
            let last = (i + 1) * 65535 >= raw.len();
            idat.push(if last { 1 } else { 0 });
            idat.extend_from_slice(&(block.len() as u16).to_le_bytes());
            idat.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            idat.extend_from_slice(block);
        }
        idat.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());

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
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(b"IDAT", &idat));
        png.extend_from_slice(&chunk(b"IEND", &[]));

        let out = optimize_png(&png).unwrap();
        assert!(out.len() < png.len() / 4, "{} vs {}", out.len(), png.len());
    }
}
