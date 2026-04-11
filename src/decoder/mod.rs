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
//! | PNG | *(always on)* | 8-bit → 8-bit AVIF; **16-bit → 10-bit AVIF** |
//! | WebP | *(always on)* | lossy and lossless |
//! | HEIC / HEIF | `heic-experimental` | Requires the `libheif` C library at link time |
//!
//! ## 16-bit PNG and HDR10
//!
//! 16-bit PNG files (a standard distribution format for HDR10 still images)
//! are decoded with full 16-bit precision and then **encoded as 10-bit AVIF**
//! via `rav1e`'s `encode_raw_planes_10_bit`.  The 6 least-significant bits are
//! discarded (>> 6), which matches the precision available in a 10-bit AV1
//! bitstream.
//!
//! The AVIF colour description (CICP metadata) will use BT.601 / sRGB
//! primaries because `rav1e` 0.7 / ravif 0.13 hardcodes those values in the
//! raw-planes encoder.  Full BT.2020 + PQ CICP metadata requires a future
//! upgrade to a newer `rav1e` build.
//!
//! HEIC files carrying HDR10 metadata are decoded via `libheif` when the
//! `heic-experimental` feature is enabled and the resulting 8-bit pixels are
//! encoded at the standard quality.

use std::io::Cursor;

use crate::Error;

/// Pixel data for a decoded image.
///
/// The 8-bit variant is produced by JPEG, WebP, 8-bit PNG, and HEIC decoders.
/// The 16-bit variant is produced by 16-bit PNG and leads to 10-bit AVIF output.
pub enum Pixels {
    /// Standard 8-bit RGBA pixels (`width × height × 4` bytes).
    Rgba8(Vec<u8>),
    /// 16-bit RGBA pixels (`width × height × 4` `u16` samples).
    ///
    /// Each sample is in the range 0 – 65 535.  The encoder scales these to
    /// the 0 – 1 023 range required by `encode_raw_planes_10_bit`.
    Rgba16(Vec<u16>),
}

/// A decoded image ready for AVIF encoding.
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel data — either 8-bit or 16-bit RGBA.
    pub pixels: Pixels,
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
/// # 16-bit PNG / HDR10
///
/// 16-bit PNG inputs are decoded with full precision and returned as
/// [`Pixels::Rgba16`].  The encoder converts these to 10-bit AVIF using
/// `encode_raw_planes_10_bit`.
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
///
/// 16-bit PNG images are decoded to [`Pixels::Rgba16`]; all other formats
/// produce [`Pixels::Rgba8`].
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
    // 16-bit RGBA needs up to `max_pixels * 8` bytes; we add 64 MiB headroom.
    let alloc_cap = max_pixels
        .saturating_mul(8)
        .saturating_add(64 * 1024 * 1024);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(alloc_cap);
    reader.limits(limits);

    let img = reader.decode().map_err(|e| Error::Decode(e.to_string()))?;

    let (width, height) = (img.width(), img.height());

    // Belt-and-suspenders pixel cap: the Limits check above should prevent
    // this, but we enforce it here too so callers always see InputTooLarge.
    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count > max_pixels {
        return Err(Error::InputTooLarge {
            width,
            height,
            max_pixels,
        });
    }

    // Preserve 16-bit precision for PNG inputs so the encoder can produce
    // genuine 10-bit AVIF output (rather than silently discarding 6 bits).
    let pixels = match img.color() {
        image::ColorType::Rgb16 | image::ColorType::Rgba16 => {
            Pixels::Rgba16(img.into_rgba16().into_raw())
        }
        _ => Pixels::Rgba8(img.into_rgba8().into_raw()),
    };

    Ok(RawImage {
        width,
        height,
        pixels,
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

    // `interleaved.stride` is the actual number of bytes per row (may include
    // padding).  Strip padding so the pixel buffer is exactly `width * 4` bytes
    // per row.
    let row_bytes = width as usize * 4;
    let stride = interleaved.stride; // bytes per row, not pixels
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
        pixels: Pixels::Rgba8(pixels),
    })
}
