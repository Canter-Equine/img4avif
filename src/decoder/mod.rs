//! Image decoding: raw bytes → [`RawImage`].
//!
//! Format is detected from magic bytes so the caller never needs to know or
//! trust a file extension.
//!
//! ## Supported formats
//!
//! | Format | Feature flag | Notes |
//! |--------|-------------|-------|
//! | JPEG / JPG | *(always on)* | 8-bit YCbCr or greyscale |
//! | PNG | *(always on)* | 8-bit and 16-bit (HDR10) inputs accepted |
//! | WebP | *(always on)* | lossy and lossless |
//! | HEIC / HEIF | `heic-experimental` | Requires the `libheif` C library at link time |
//!
//! ## HDR10 notes
//!
//! 16-bit PNG files (often used to store HDR10 content) are accepted by the
//! PNG decoder.  The `image` crate scales each channel from 16-bit to 8-bit
//! before the pixel buffer is handed to the AVIF encoder, so the output is
//! an SDR AVIF.  True 10-bit HDR AVIF output with BT.2020 / PQ colour
//! metadata requires a future upgrade to the encoder backend.
//!
//! HEIC files carrying HDR10 metadata are decoded via `libheif` when the
//! `heic-experimental` feature is enabled.  The HDR colour profile is
//! preserved in `libheif`'s decoded bitmap; the AVIF encoder then encodes
//! the resulting RGBA8 pixels.

use std::io::Cursor;

use crate::Error;

/// A decoded image in row-major RGBA8 format, ready for AVIF encoding.
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw pixel data: `width × height × 4` bytes, in RGBA order.
    pub pixels: Vec<u8>,
}

/// Returns `true` when `data` starts with an ISO Base Media file type box
/// (`ftyp`), which is the common container for HEIF and HEIC files.
///
/// The ISOBMFF `ftyp` box layout is:
/// ```text
/// [ 4 bytes: box size ][ 4 bytes: b"ftyp" ][ 4 bytes: major brand ] ...
/// ```
fn is_heif_ftyp(data: &[u8]) -> bool {
    data.len() >= 12 && data[4..8] == *b"ftyp"
}

/// Decode `data` into a [`RawImage`].
///
/// Format is detected from the file's magic bytes.  JPEG, PNG, and WebP are
/// always supported.  HEIC / HEIF requires the `heic-experimental` feature.
///
/// The decoder allocation budget is capped at `max_pixels * 4 + 64 MiB` to
/// prevent decompression-bomb attacks: a small compressed file that claims
/// enormous dimensions will exhaust the budget and return an error rather
/// than allocating gigabytes of RAM.
///
/// # HDR10
///
/// 16-bit PNG inputs are accepted and the channels are scaled to 8-bit before
/// encoding.  HEIC files with HDR10 metadata are handled by `libheif` when
/// the `heic-experimental` feature is enabled.
///
/// # Errors
///
/// - [`Error::Decode`] — malformed or truncated input.
/// - [`Error::InputTooLarge`] — decoded dimensions exceed `max_pixels`.
/// - [`Error::UnsupportedFormat`] — format detected but not supported (e.g.,
///   HEIC/HEIF without the `heic-experimental` feature enabled).
pub fn decode(data: &[u8], max_pixels: u64) -> Result<RawImage, Error> {
    // Route HEIC/HEIF through the libheif decoder when available.
    if is_heif_ftyp(data) {
        return decode_heif(data, max_pixels);
    }

    decode_via_image_crate(data, max_pixels)
}

/// Decode a JPEG, PNG, or WebP image using the `image` crate.
fn decode_via_image_crate(data: &[u8], max_pixels: u64) -> Result<RawImage, Error> {
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

/// Decode a HEIC / HEIF image.
///
/// When the `heic-experimental` feature is enabled this uses `libheif-rs` to
/// decode the image into RGBA8 pixels.  When the feature is absent an
/// [`Error::UnsupportedFormat`] is returned immediately with instructions on
/// how to enable support.
fn decode_heif(data: &[u8], max_pixels: u64) -> Result<RawImage, Error> {
    #[cfg(feature = "heic-experimental")]
    return decode_heif_impl(data, max_pixels);

    #[cfg(not(feature = "heic-experimental"))]
    {
        let _ = (data, max_pixels); // suppress unused-variable warnings
        Err(Error::UnsupportedFormat(
            "HEIC/HEIF (enable the `heic-experimental` Cargo feature and \
             ensure `libheif` is installed on the system)"
                .into(),
        ))
    }
}

/// Actual `libheif`-backed HEIC/HEIF decoder, compiled only when the feature
/// is enabled.
#[cfg(feature = "heic-experimental")]
fn decode_heif_impl(data: &[u8], max_pixels: u64) -> Result<RawImage, Error> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let _lib = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(data)
        .map_err(|e| Error::Decode(format!("HEIF context: {e}")))?;

    let handle = ctx
        .primary_image_handle()
        .map_err(|e| Error::Decode(format!("HEIF primary image: {e}")))?;

    // Enforce the pixel budget before allocating the decode buffer.
    let width = handle.width();
    let height = handle.height();
    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count > max_pixels {
        return Err(Error::InputTooLarge {
            width,
            height,
            max_pixels,
        });
    }

    // Decode to interleaved RGBA8.
    let image = _lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|e| Error::Decode(format!("HEIF decode: {e}")))?;

    let planes = image.planes();
    let interleaved = planes
        .interleaved
        .ok_or_else(|| Error::Decode("HEIF image has no interleaved RGBA plane".into()))?;

    // The pixel count has already been validated above, so the stride may
    // include padding bytes at the end of each row.  Strip them out so that
    // the pixel buffer is exactly `width * height * 4` bytes.
    let row_bytes = width as usize * 4;
    let stride = interleaved.width as usize * 4;
    let pixels: Vec<u8> = if stride == row_bytes {
        interleaved.data.to_vec()
    } else {
        interleaved
            .data
            .chunks(stride)
            .flat_map(|row| &row[..row_bytes])
            .copied()
            .collect()
    };

    Ok(RawImage {
        width,
        height,
        pixels,
    })
}
