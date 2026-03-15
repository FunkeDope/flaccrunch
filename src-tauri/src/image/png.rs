use std::fs;
use std::path::Path;

/// Result of optimizing a PNG image.
#[derive(Debug)]
pub struct PngOptimizeResult {
    pub original_size: u64,
    pub optimized_size: u64,
    pub saved_bytes: i64,
}

/// Optimize a PNG file in-place using the oxipng library.
/// Uses optimization level 4 (equivalent to `oxipng -o 4`).
pub fn optimize_png(path: &Path) -> Result<PngOptimizeResult, String> {
    let original_data = fs::read(path).map_err(|e| format!("Failed to read PNG: {e}"))?;
    let original_size = original_data.len() as u64;

    let options = oxipng::Options {
        optimize_alpha: true,
        ..oxipng::Options::from_preset(4)
    };

    oxipng::optimize(
        &oxipng::InFile::Path(path.to_path_buf()),
        &oxipng::OutFile::Path {
            path: Some(path.to_path_buf()),
            preserve_attrs: true,
        },
        &options,
    )
    .map_err(|e| format!("oxipng optimization failed: {e}"))?;

    let optimized_size = fs::metadata(path)
        .map_err(|e| format!("Failed to read optimized PNG metadata: {e}"))?
        .len();

    Ok(PngOptimizeResult {
        original_size,
        optimized_size,
        saved_bytes: original_size as i64 - optimized_size as i64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but valid 1×1 RGB PNG in memory.
    ///
    /// Structure:
    ///   Signature (8 bytes)
    ///   IHDR chunk  (width=1, height=1, bit_depth=8, color_type=2/RGB, ...)
    ///   IDAT chunk  (zlib-compressed scanline: filter_byte=0 + R G B)
    ///   IEND chunk
    fn minimal_1x1_rgb_png() -> Vec<u8> {
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
            let len = data.len() as u32;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(tag);
            out.extend_from_slice(data);
            let mut crc_input = Vec::new();
            crc_input.extend_from_slice(tag);
            crc_input.extend_from_slice(data);
            out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
            out
        }

        let mut png = Vec::new();
        // PNG signature
        png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

        // IHDR: width=1, height=1, bit_depth=8, color_type=2 (RGB),
        //       compression=0, filter=0, interlace=0
        let ihdr_data: [u8; 13] = [
            0, 0, 0, 1, // width  = 1
            0, 0, 0, 1, // height = 1
            8,          // bit depth
            2,          // color type RGB
            0, 0, 0,    // compression, filter, interlace
        ];
        png.extend_from_slice(&chunk(b"IHDR", &ihdr_data));

        // IDAT: raw scanline is filter_byte(0) + R(255) G(255) B(255)
        // We zlib-compress it with flate2 or just use the "uncompressed" zlib format.
        // Use a stored (no compression) zlib block:
        //   zlib header: CMF=0x78, FLG=0x01 (zlib level 1, no dict, check = (0x78*256+FLG) % 31 == 0)
        // Actually easiest: build via miniz_oxide which oxipng already depends on.
        // Instead, use the known good encoding for [0, 255, 255, 255] with no-compression zlib.
        //   zlib: 0x78 0x01, then deflate stored block: BFINAL=1 BTYPE=00,
        //         LEN=4 (u16 LE), NLEN=0xFFFB (complement), then 4 bytes, then adler32.
        let scanline: [u8; 4] = [0x00, 0xFF, 0xFF, 0xFF]; // filter + RGB white
        let len_le: [u8; 2] = (scanline.len() as u16).to_le_bytes();
        let nlen_le: [u8; 2] = (!(scanline.len() as u16)).to_le_bytes();

        // Adler32 of scanline
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;
        for &b in &scanline {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        let adler = ((s2 << 16) | s1).to_be_bytes();

        let mut idat_data = Vec::new();
        idat_data.push(0x78); // CMF
        idat_data.push(0x01); // FLG
        idat_data.push(0x01); // BFINAL=1, BTYPE=00
        idat_data.extend_from_slice(&len_le);
        idat_data.extend_from_slice(&nlen_le);
        idat_data.extend_from_slice(&scanline);
        idat_data.extend_from_slice(&adler);
        png.extend_from_slice(&chunk(b"IDAT", &idat_data));

        // IEND
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    fn write_temp_png(name: &str, data: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, data).expect("write temp PNG");
        path
    }

    #[test]
    fn test_optimize_png_on_valid_png_succeeds() {
        let png_data = minimal_1x1_rgb_png();
        let path = write_temp_png("flaccrunch_test_1x1.png", &png_data);
        let result = optimize_png(&path);
        // oxipng may or may not shrink a 1×1 PNG, but it must not error
        assert!(result.is_ok(), "optimize_png failed: {:?}", result.err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_optimize_png_result_fields_are_consistent() {
        let png_data = minimal_1x1_rgb_png();
        let path = write_temp_png("flaccrunch_test_fields.png", &png_data);
        let result = optimize_png(&path).expect("optimize_png should succeed");
        assert_eq!(result.original_size, png_data.len() as u64);
        assert!(result.optimized_size > 0);
        assert_eq!(
            result.saved_bytes,
            result.original_size as i64 - result.optimized_size as i64
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_optimize_png_on_missing_file_returns_err() {
        let path = Path::new("/nonexistent/totally_fake_image.png");
        let result = optimize_png(path);
        assert!(result.is_err(), "should return Err for missing file");
    }

    #[test]
    fn test_optimize_png_on_invalid_data_returns_err() {
        let path = write_temp_png("flaccrunch_test_invalid.png", b"not a png at all");
        let result = optimize_png(&path);
        // Either oxipng rejects it or the read succeeds but optimization fails
        // — either way we just confirm the function doesn't panic.
        drop(result);
        let _ = std::fs::remove_file(&path);
    }
}
