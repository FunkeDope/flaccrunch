use serde::{Deserialize, Serialize};

/// Supported image formats for optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Detect the image format by examining magic bytes (file signature).
pub fn detect_image_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() < 4 {
        return None;
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(ImageFormat::Png);
    }

    // JPEG: FF D8 FF
    if data[..3] == [0xFF, 0xD8, 0xFF] {
        return Some(ImageFormat::Jpeg);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(detect_image_format(&data), Some(ImageFormat::Png));
    }

    #[test]
    fn test_detect_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(detect_image_format(&data), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn test_detect_unknown() {
        // BMP magic: 42 4D
        let data = [0x42, 0x4D, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(detect_image_format(&data), None);
    }

    #[test]
    fn test_detect_too_short() {
        let data = [0x89, 0x50];
        assert_eq!(detect_image_format(&data), None);
    }

    #[test]
    fn test_detect_empty() {
        let data: [u8; 0] = [];
        assert_eq!(detect_image_format(&data), None);
    }
}
