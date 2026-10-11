pub mod detect;
pub mod jpeg;
pub mod png;

use detect::{detect_image_format, ImageFormat};

/// Decoded form of a picture, used to prove two encodings are the same image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedImage {
    Png(png::DecodedPng),
    /// (width, height, RGB bytes)
    Jpeg(usize, usize, Vec<u8>),
}

/// Decode a PNG or JPEG. `None` for anything else or undecodable data.
pub fn decode_image(data: &[u8]) -> Option<DecodedImage> {
    match detect_image_format(data)? {
        ImageFormat::Png => png::decode_png(data).map(DecodedImage::Png),
        ImageFormat::Jpeg => {
            jpeg::decode_pixels(data).map(|(w, h, px)| DecodedImage::Jpeg(w, h, px))
        }
    }
}
