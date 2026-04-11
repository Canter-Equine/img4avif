//! Image decoding: raw bytes → [`RawImage`].
//!
//! Format is detected from magic bytes so the caller never needs to know or
//! trust a file extension.

use std::io::Cursor;

use crate::Error;

/// A decoded image in row-major RGBA8 format, ready for AVIF encoding.
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw pixel data: `width × height × 4` bytes.
    pub pixels: Vec<u8>,
}

/// Decode `data` into a [`RawImage`].
///
/// The decoder allocation budget is capped at `max_pixels * 4 + 64 MiB` to
/// prevent decompression-bomb attacks: a small compressed file that claims
/// enormous dimensions will exhaust the budget and return an error rather
/// than allocating gigabytes of RAM.
///
/// # Errors
///
/// - [`Error::Decode`] — malformed or truncated input.
/// - [`Error::InputTooLarge`] — decoded dimensions exceed `max_pixels`.
/// - [`Error::UnsupportedFormat`] — format detected but not supported.
pub fn decode(data: &[u8], max_pixels: u64) -> Result<RawImage, Error> {
    let mut reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| Error::Decode(e.to_string()))?;

    // Reject formats we don't support before touching the pixel data.
    match reader.format() {
        Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP) => {}
        Some(other) => return Err(Error::UnsupportedFormat(format!("{other:?}"))),
        None => return Err(Error::Decode("unrecognised image format".into())),
    }

    // Cap the decoder's allocation budget to prevent decompression bombs.
    // A legitimate image of `max_pixels` RGBA8 pixels costs `max_pixels * 4`
    // bytes; we add 64 MiB headroom for decoder internal state.
    let alloc_cap = max_pixels
        .saturating_mul(4)
        .saturating_add(64 * 1024 * 1024);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(alloc_cap);
    reader.limits(limits);

    let img = reader.decode().map_err(|e| Error::Decode(e.to_string()))?;
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();

    // Belt-and-suspenders: the Limits check above should prevent this, but
    // we enforce the pixel cap here too so callers always see InputTooLarge
    // rather than a cryptic Decode error if a decoder ignores max_alloc.
    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count > max_pixels {
        return Err(Error::InputTooLarge {
            width,
            height,
            max_pixels,
        });
    }

    Ok(RawImage {
        width,
        height,
        pixels: rgba.into_raw(),
    })
}
