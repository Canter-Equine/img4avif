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

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::OnceLock;

use crate::logging::{img_debug, img_error, img_info};
use crate::Error;

/// Pixel data for a decoded image.
///
/// The 8-bit variant is produced by JPEG, WebP, 8-bit PNG, and HEIC decoders.
/// The 16-bit variant is produced by 16-bit PNG and leads to 10-bit AVIF output.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel data — either 8-bit or 16-bit RGBA.
    pub pixels: Pixels,
}

/// Returns a reference to the lazily-initialised set of recognised HEIF/HEIC
/// major brand codes.
///
/// Initialised exactly once on first call via [`OnceLock`].  Subsequent calls
/// return a reference to the same allocation with no synchronisation overhead.
/// This pattern replaces a linear scan over a const slice with an O(1) hash
/// lookup, and avoids repeating the brand list in every call site.
fn heif_brands() -> &'static HashSet<[u8; 4]> {
    static BRANDS: OnceLock<HashSet<[u8; 4]>> = OnceLock::new();
    BRANDS.get_or_init(|| {
        [
            *b"heic", // HEVC Main still image
            *b"heis", // HEVC Main still image (scalable)
            *b"hevc", // HEVC Main image sequence
            *b"hevx", // HEVC Main + extensions image sequence
            *b"heim", // HEVC still image with multi-layer
            *b"heix", // HEVC still image with extensions
            *b"mif1", // Image items (including AVIF-as-HEIF)
            *b"msf1", // Image sequence (including HEIF video)
            *b"avif", // AVIF still image
        ]
        .into_iter()
        .collect()
    })
}

/// Returns `true` when `data` starts with an ISO Base Media file type box
/// (`ftyp`) **and** the major brand identifies a HEIF/HEIC family container.
///
/// The ISOBMFF `ftyp` box layout is:
/// ```text
/// [ 4 bytes: box size ][ 4 bytes: b"ftyp" ][ 4 bytes: major brand ] ...
/// ```
///
/// Checking only the `ftyp` marker is not sufficient because many other
/// ISOBMFF-based formats (MP4, MOV, M4A, CMAF …) also start with `ftyp`.
/// We therefore also verify that the 4-byte major brand is one of the known
/// HEIF family brands before routing the file to the HEIF decoder.
fn is_heif_ftyp(data: &[u8]) -> bool {
    if data.len() < 12 || data[4..8] != *b"ftyp" {
        return false;
    }
    // JUSTIFICATION: data[8..12] is guaranteed to be exactly 4 bytes because
    // the length check above confirms data.len() >= 12.
    let brand: [u8; 4] = data[8..12]
        .try_into()
        .expect("data[8..12] must be exactly 4 bytes — guaranteed by the len >= 12 check above");
    heif_brands().contains(&brand)
}

/// Decode `data` into a [`RawImage`].
///
/// Format is detected from the file's magic bytes.  JPEG, PNG, and WebP are
/// always supported.  HEIC / HEIF requires the `heic-experimental` feature.
///
/// The decoder allocation budget is capped at `max_pixels * 8 + 64 MiB` to
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
    img_debug!("decode: {} bytes, max_pixels={}", data.len(), max_pixels);

    // Route HEIC/HEIF through the libheif decoder when available.
    if is_heif_ftyp(data) {
        img_info!("decode: detected HEIC/HEIF container (ftyp magic)");
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
        Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP) => {
            img_debug!("decode: detected format {:?}", reader.format());
        }
        Some(other) => {
            img_error!("decode: unsupported format {:?}", other);
            return Err(Error::UnsupportedFormat(format!("{other:?}")));
        }
        None => {
            img_error!("decode: could not detect image format");
            return Err(Error::Decode("unrecognised image format".into()));
        }
    }

    // Cap the decoder's allocation budget to prevent decompression bombs.
    // 16-bit RGBA needs up to `max_pixels * 8` bytes; we add 64 MiB headroom.
    let alloc_cap = max_pixels
        .saturating_mul(8)
        .saturating_add(64 * 1024 * 1024);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(alloc_cap);
    reader.limits(limits);

    let img = reader.decode().map_err(|e| {
        img_error!("decode: image decode error: {}", e);
        Error::Decode(e.to_string())
    })?;

    let (width, height) = (img.width(), img.height());
    let pixel_count = u64::from(width) * u64::from(height);

    img_debug!(
        "decode: raw dimensions {}×{} ({} Mpx), colour type={:?}",
        width,
        height,
        pixel_count / 1_000_000,
        img.color()
    );

    // Belt-and-suspenders pixel cap: the Limits check above should prevent
    // this, but we enforce it here too so callers always see InputTooLarge.
    if pixel_count > max_pixels {
        img_error!(
            "decode: image {}×{} ({} px) exceeds max_pixels={}",
            width,
            height,
            pixel_count,
            max_pixels
        );
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
            img_info!("decode: 16-bit PNG detected — preserving full precision for 10-bit AVIF");
            Pixels::Rgba16(img.into_rgba16().into_raw())
        }
        _ => {
            img_debug!("decode: converting to RGBA8");
            Pixels::Rgba8(img.into_rgba8().into_raw())
        }
    };

    img_info!("decode: {}×{} decoded OK", width, height);

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
        img_error!("decode: HEIC/HEIF input but `heic-experimental` feature is not enabled");
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
    let ctx = HeifContext::read_from_bytes(data).map_err(|e| {
        img_error!("decode_heif: context parse error: {}", e);
        Error::Decode(format!("HEIF context: {e}"))
    })?;

    let handle = ctx.primary_image_handle().map_err(|e| {
        img_error!("decode_heif: could not get primary image handle: {}", e);
        Error::Decode(format!("HEIF primary image: {e}"))
    })?;

    // Enforce the pixel budget before allocating the decode buffer.
    let width = handle.width();
    let height = handle.height();
    let pixel_count = u64::from(width) * u64::from(height);

    img_debug!(
        "decode_heif: {}×{} ({} Mpx)",
        width,
        height,
        pixel_count / 1_000_000
    );

    if pixel_count > max_pixels {
        img_error!(
            "decode_heif: {}×{} ({} px) exceeds max_pixels={}",
            width,
            height,
            pixel_count,
            max_pixels
        );
        return Err(Error::InputTooLarge {
            width,
            height,
            max_pixels,
        });
    }

    // Decode to interleaved RGBA8.
    let image = _lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|e| {
            img_error!("decode_heif: pixel decode error: {}", e);
            Error::Decode(format!("HEIF decode: {e}"))
        })?;

    let planes = image.planes();
    let interleaved = planes.interleaved.ok_or_else(|| {
        img_error!("decode_heif: no interleaved RGBA plane in decoded image");
        Error::Decode("HEIF image has no interleaved RGBA plane".into())
    })?;

    let pixels =
        heif_interleaved_to_rgba_pixels(interleaved.data, width, height, interleaved.stride)
            .map_err(|e| {
                img_error!("decode_heif: malformed interleaved plane: {}", e);
                e
            })?;

    img_info!("decode_heif: {}×{} decoded OK", width, height);

    Ok(RawImage {
        width,
        height,
        pixels: Pixels::Rgba8(pixels),
    })
}

#[cfg(any(feature = "heic-experimental", test))]
fn heif_interleaved_to_rgba_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<Vec<u8>, Error> {
    // `stride` is bytes per row (may include padding). We require enough bytes
    // to safely copy exactly `width * 4` bytes per row without panicking.
    let row_bytes = width as usize * 4;
    let rows = height as usize;

    if stride < row_bytes {
        return Err(Error::Decode(format!(
            "HEIF interleaved stride {stride} is smaller than row size {row_bytes}"
        )));
    }

    let expected_len = stride
        .checked_mul(rows)
        .ok_or_else(|| Error::Decode("HEIF interleaved plane size overflow".into()))?;
    if data.len() < expected_len {
        return Err(Error::Decode(format!(
            "HEIF interleaved plane too short: got {} bytes, expected at least {} \
             for {} rows with stride {}",
            data.len(),
            expected_len,
            rows,
            stride
        )));
    }

    if stride == row_bytes {
        return Ok(data[..expected_len].to_vec());
    }

    img_debug!(
        "decode_heif: row stride {} != expected {} — stripping per-row padding",
        stride,
        row_bytes
    );

    let out_len = row_bytes
        .checked_mul(rows)
        .ok_or_else(|| Error::Decode("HEIF RGBA output size overflow".into()))?;
    let mut pixels = Vec::with_capacity(out_len);
    for y in 0..rows {
        let row_start = y * stride;
        let row_end = row_start + row_bytes;
        pixels.extend_from_slice(&data[row_start..row_end]);
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heif_stride_smaller_than_row_is_decode_error() {
        let err = heif_interleaved_to_rgba_pixels(&[0; 8], 3, 1, 8).unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[test]
    fn heif_short_plane_is_decode_error() {
        let err = heif_interleaved_to_rgba_pixels(&[0; 11], 2, 2, 6).unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[test]
    fn heif_valid_padding_layout_is_compacted() {
        let src = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 9, // row 0 (8 bytes RGBA + 2 bytes padding)
            10, 11, 12, 13, 14, 15, 16, 17, 8, 8, // row 1
        ];
        let out = heif_interleaved_to_rgba_pixels(&src, 2, 2, 10).unwrap();
        assert_eq!(
            out,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17]
        );
    }
}
