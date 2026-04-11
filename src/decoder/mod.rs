//! Image decoding: raw bytes → [`RawImage`].
//!
//! Format detection is performed by sniffing magic bytes, not by file
//! extension, making it safe to use with untrusted input.

pub mod jpeg;
pub mod png;
pub mod webp;

use crate::Error;

/// A decoded image in row-major **RGBA8** format, ready for AVIF encoding.
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw pixel data: `width × height × 4` bytes, row-major RGBA.
    pub pixels: Vec<u8>,
}

/// Decoder trait for per-format implementations.
///
/// Concrete decoders live in the sibling modules (`jpeg`, `png`, `webp`).
/// The top-level [`decode`] function dispatches to the right decoder
/// automatically based on magic bytes.
#[allow(dead_code)]
pub trait Decoder {
    /// Decode `data` into a [`RawImage`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Decode`] on malformed input.
    fn decode(&self, data: &[u8]) -> Result<RawImage, Error>;
}

/// Decode `data` into a [`RawImage`] by sniffing the format from magic bytes.
///
/// Supported formats: **JPEG**, **PNG**, **WebP**.
///
/// # Errors
///
/// - [`Error::Decode`] — bytes are not a valid image of any supported format.
/// - [`Error::UnsupportedFormat`] — format was detected but is not enabled in
///   this build (e.g. HEIC without the `heic-experimental` feature).
pub fn decode(data: &[u8]) -> Result<RawImage, Error> {
    let format =
        image::guess_format(data).map_err(|e| Error::Decode(format!("unknown format: {e}")))?;

    match format {
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP => {}
        other => {
            return Err(Error::UnsupportedFormat(format!("{other:?}")));
        }
    }

    let img = image::load_from_memory(data).map_err(|e| Error::Decode(e.to_string()))?;

    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();

    Ok(RawImage {
        width,
        height,
        pixels,
    })
}
