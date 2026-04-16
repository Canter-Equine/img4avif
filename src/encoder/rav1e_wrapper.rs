//! AVIF encoding via the `ravif` / `rav1e` pure-Rust AV1 encoder.
//!
//! This module wraps [`ravif::Encoder`] and converts between our internal
//! [`RawImage`] representation and the types expected by `ravif`.
//!
//! ## Bit-depth selection
//!
//! | Input | Output |
//! |-------|--------|
//! | 8-bit RGBA ([`Pixels::Rgba8`]) | `encode_rgba` → 10-bit AVIF (ravif auto-selects) |
//! | 16-bit RGBA ([`Pixels::Rgba16`]) | `encode_raw_planes_10_bit` → 10-bit AVIF |
//!
//! The 16-bit path converts each RGBA16 channel (0 – 65 535) to the 10-bit
//! range (0 – 1 023) by right-shifting six bits, then converts to YCbCr using
//! the BT.601 matrix.  This matches the colour model used by the 8-bit path
//! so there is no colour-space discontinuity when mixing input depths.
//!
//! ## Output validation
//!
//! After encoding, the raw bytes are validated:
//!
//! 1. The output must be non-empty.
//! 2. The output must be at least 20 bytes (the minimum size of a valid ISOBMFF
//!    `ftyp` box).
//! 3. Bytes 4–7 must equal `ftyp` — the ISOBMFF box-type marker present in
//!    every well-formed AVIF file.
//! 4. Bytes 8–11 must equal `avif` or `avis` — the AVIF major brand.
//! 5. Bytes 0–3 must encode a box size ≥ 20 and ≤ total output length.
//!
//! If any check fails, [`Error::Encode`] is returned with a descriptive
//! message instead of silently handing back corrupt bytes to the caller.

use crate::decoder::{Pixels, RawImage};
use crate::logging::{img_debug, img_error, img_info, img_warn};
use crate::Error;

/// Minimum byte length of a structurally valid AVIF file.
///
/// An AVIF file is an ISOBMFF container.  The outermost box is always `ftyp`
/// whose minimum layout is:
/// ```text
/// [ 4 bytes size ][ 4 bytes "ftyp" ][ 4 bytes major brand ]
/// [ 4 bytes minor version ][ 4 bytes compatible brand × 1 ]
/// ```
/// That totals 20 bytes.
const MIN_AVIF_BYTES: usize = 20;

/// Encode a [`RawImage`] as AVIF.
///
/// - For **8-bit** inputs (`Pixels::Rgba8`): calls `ravif::Encoder::encode_rgba`.
///   `rav1e` auto-selects 10-bit encoding internally for better quality.
/// - For **16-bit** inputs (`Pixels::Rgba16`): calls
///   `ravif::Encoder::encode_raw_planes_10_bit` so the full 10-bit precision
///   is preserved rather than being silently discarded.
///
/// `quality` must be in **1 – 10** (higher = better). Scaled to 1-100 for ravif.
/// `speed` must be in **1 – 10** (higher = faster).
/// `alpha_quality` must be in **1 – 10**; pass the same value as `quality`
/// for uniform quality, or a higher value (e.g. 10) to keep the alpha channel
/// visually lossless.
///
/// # Output validation
///
/// The encoded bytes are validated against the AVIF / ISOBMFF container
/// format before being returned.  [`Error::Encode`] is returned if the
/// encoder produces empty, truncated, structurally invalid output, or output
/// with an unexpected major brand or invalid box size.
///
/// # Errors
///
/// Returns [`Error::Encode`] if `rav1e` fails or produces invalid output.
pub fn encode_avif(
    image: &RawImage,
    quality: u8,
    speed: u8,
    alpha_quality: u8,
) -> Result<Vec<u8>, Error> {
    // Check if the image has any transparency
    let has_transparency = image.has_transparency();
    
    img_debug!(
        "encode_avif: {}×{} px, quality={}, alpha_quality={}, speed={}, depth={}, transparency={}",
        image.width,
        image.height,
        quality,
        alpha_quality,
        speed,
        match &image.pixels {
            Pixels::Rgba8(_) => "8-bit",
            Pixels::Rgba16(_) => "16-bit",
        },
        has_transparency
    );

    // Scale quality from 1-10 range to 1-100 range for ravif
    let ravif_quality = (u32::from(quality.clamp(1, 10)) * 10).min(100) as u8;
    
    // Only use alpha_quality if the image has transparency; otherwise use quality
    let ravif_alpha_quality = if has_transparency {
        (u32::from(alpha_quality.clamp(1, 10)) * 10).min(100) as u8
    } else {
        img_debug!("encode_avif: no transparency detected, treating alpha_quality as no-op");
        ravif_quality
    };

    let avif = match &image.pixels {
        Pixels::Rgba8(bytes) => encode_8bit(
            image.width,
            image.height,
            bytes,
            ravif_quality,
            speed,
            ravif_alpha_quality,
        ),
        Pixels::Rgba16(samples) => encode_16bit(
            image.width,
            image.height,
            samples,
            ravif_quality,
            speed,
            ravif_alpha_quality,
        ),
    }?;

    validate_avif_output(&avif, image.width, image.height)?;

    #[cfg(feature = "dev-logging")]
    img_info!(
        "encode_avif: produced {} bytes ({:.1}× compression ratio)",
        avif.len(),
        compression_ratio(image, avif.len()),
    );
    #[cfg(not(feature = "dev-logging"))]
    img_info!("encode_avif: produced {} bytes", avif.len());

    Ok(avif)
}

/// Validate that `bytes` looks like a structurally sound AVIF file.
///
/// Checks:
/// 1. Non-empty.
/// 2. At least [`MIN_AVIF_BYTES`] long.
/// 3. Bytes 4–7 are `b"ftyp"` — the ISOBMFF file-type box marker.
/// 4. Bytes 8–11 are `b"avif"` or `b"avis"` — the AVIF major brand.
/// 5. Bytes 0–3 encode a box size ≥ 20 and ≤ the total output length.
///
/// These checks are lightweight (no full ISOBMFF parse) and catch the most
/// common failure modes: empty output, truncated output, and the encoder
/// accidentally emitting raw bitstream data without wrapping it in a container.
fn validate_avif_output(bytes: &[u8], width: u32, height: u32) -> Result<(), Error> {
    if bytes.is_empty() {
        img_error!(
            "encode_avif: encoder returned empty output for {}×{} image",
            width,
            height
        );
        return Err(Error::Encode(
            "AVIF encoder produced empty output — this is a bug; please report it".into(),
        ));
    }

    if bytes.len() < MIN_AVIF_BYTES {
        img_error!(
            "encode_avif: output too short ({} bytes, expected ≥ {}) for {}×{} image",
            bytes.len(),
            MIN_AVIF_BYTES,
            width,
            height
        );
        return Err(Error::Encode(format!(
            "AVIF encoder produced truncated output ({} bytes, minimum valid AVIF is {} bytes)",
            bytes.len(),
            MIN_AVIF_BYTES,
        )));
    }

    if bytes[4..8] != *b"ftyp" {
        // Log as hex so developers can identify what the encoder actually returned.
        img_error!(
            "encode_avif: output missing ISOBMFF 'ftyp' box — bytes[0..12] = {:02x?}",
            &bytes[..bytes.len().min(12)]
        );
        return Err(Error::Encode(format!(
            "AVIF encoder produced invalid container: expected ISOBMFF 'ftyp' box at offset 4, \
             got {:02x?}",
            &bytes[4..8],
        )));
    }

    // Verify the AVIF major brand (bytes 8–11).
    if bytes[8..12] != *b"avif" && bytes[8..12] != *b"avis" {
        img_error!(
            "encode_avif: unexpected major brand — bytes[8..12] = {:02x?}",
            &bytes[8..12]
        );
        return Err(Error::Encode(format!(
            "AVIF major brand invalid: expected 'avif' or 'avis', got {:02x?}",
            &bytes[8..12],
        )));
    }

    // Verify the ftyp box size field (bytes 0–3, big-endian u32).
    let box_size = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if box_size < MIN_AVIF_BYTES || box_size > bytes.len() {
        img_error!(
            "encode_avif: ftyp box size {} is invalid (output is {} bytes)",
            box_size,
            bytes.len()
        );
        return Err(Error::Encode(format!(
            "AVIF ftyp box size invalid: box_size={box_size}, output length={}",
            bytes.len()
        )));
    }

    img_debug!(
        "encode_avif: output validation passed — {} bytes with ftyp box",
        bytes.len()
    );

    // Warn if the file is suspiciously small relative to the pixel count.
    // A valid AVIF for a non-trivial image is almost always > 100 bytes; a
    // very low value could indicate that the encoder silently skipped the
    // image data.
    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count > 64 && bytes.len() < 100 {
        img_warn!(
            "encode_avif: output is suspiciously small ({} bytes) for a {}×{} image — \
             verify the AVIF is decodable",
            bytes.len(),
            width,
            height
        );
    }

    Ok(())
}

/// Approximate compression ratio: `input_bytes / output_bytes`.
#[cfg(feature = "dev-logging")]
fn compression_ratio(image: &RawImage, output_bytes: usize) -> f64 {
    let input_bytes: u64 = match &image.pixels {
        Pixels::Rgba8(b) => b.len() as u64,
        Pixels::Rgba16(s) => s.len() as u64 * 2,
    };
    if output_bytes == 0 {
        return 0.0;
    }
    // Use u64 → f64; safe for any realistic image size (well under 2^52 bytes).
    #[allow(clippy::cast_precision_loss)]
    {
        input_bytes as f64 / output_bytes as f64
    }
}

/// Encode 8-bit RGBA pixels using `ravif::Encoder::encode_rgba`.
fn encode_8bit(
    width: u32,
    height: u32,
    pixels: &[u8],
    quality: u8,
    speed: u8,
    alpha_quality: u8,
) -> Result<Vec<u8>, Error> {
    use ravif::{EncodedImage, Encoder, Img};
    use rgb::FromSlice;

    img_debug!(
        "encode_8bit: {}×{} RGBA8 → rav1e encode_rgba",
        width,
        height
    );

    let rgba = pixels.as_rgba();
    let img = Img::new(rgba, width as usize, height as usize);

    Encoder::new()
        .with_quality(f32::from(quality.clamp(1, 100)))
        .with_alpha_quality(f32::from(alpha_quality.clamp(1, 100)))
        .with_speed(speed.clamp(1, 10))
        .encode_rgba(img)
        .map(|EncodedImage { avif_file, .. }| avif_file)
        .map_err(|e| {
            img_error!("encode_8bit: rav1e failed: {}", e);
            Error::Encode(e.to_string())
        })
}

/// Encode 16-bit RGBA pixels as 10-bit AVIF using `ravif::Encoder::encode_raw_planes_10_bit`.
///
/// Each 16-bit channel (0 – 65 535) is scaled to 10-bit (0 – 1 023) by
/// discarding the bottom 6 bits, then converted to YCbCr with the BT.601
/// luma matrix to match `ravif`'s standard encoding path.
fn encode_16bit(
    width: u32,
    height: u32,
    pixels: &[u16],
    quality: u8,
    speed: u8,
    alpha_quality: u8,
) -> Result<Vec<u8>, Error> {
    use ravif::{EncodedImage, Encoder, MatrixCoefficients, PixelRange};

    let width_usize = width as usize;
    let height_usize = height as usize;

    img_debug!(
        "encode_16bit: {}×{} RGBA16 → rav1e encode_raw_planes_10_bit (BT.601 YCbCr)",
        width,
        height
    );

    // Each pixel is [R, G, B, A] as u16 (0-65535).
    // Convert to 10-bit YCbCr planes and a separate alpha plane.
    let mut ycbcr_planes: Vec<[u16; 3]> = Vec::with_capacity(width_usize * height_usize);
    let mut alpha_plane: Vec<u16> = Vec::with_capacity(width_usize * height_usize);

    for chunk in pixels.chunks_exact(4) {
        let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        ycbcr_planes.push(rgba16_to_10bit_ycbcr_bt601(r, g, b));
        // Scale alpha from 16-bit to 10-bit.
        alpha_plane.push(a >> 6);
    }

    Encoder::new()
        .with_quality(f32::from(quality.clamp(1, 100)))
        .with_alpha_quality(f32::from(alpha_quality.clamp(1, 100)))
        .with_speed(speed.clamp(1, 10))
        .encode_raw_planes_10_bit(
            width_usize,
            height_usize,
            ycbcr_planes,
            Some(alpha_plane),
            PixelRange::Full,
            MatrixCoefficients::BT601,
        )
        .map(|EncodedImage { avif_file, .. }| avif_file)
        .map_err(|e| {
            img_error!("encode_16bit: rav1e failed: {}", e);
            Error::Encode(e.to_string())
        })
}

/// Convert a 16-bit RGB triplet (0 – 65 535) to 10-bit YCbCr using the
/// BT.601 luma coefficients (Kr = 0.2990, Kg = 0.5870, Kb = 0.1140).
///
/// This mirrors the formula used inside `ravif`'s `rgb_to_10_bit_ycbcr`
/// function, extended to 16-bit input so the full precision of 16-bit PNG
/// files is preserved in the 10-bit AVIF stream.
///
/// # Precision
///
/// The final `.clamp(0.0, 1023.0)` call mathematically guarantees that every
/// output value is in `[0.0, 1023.0]` before the `as u32 as u16` cast.
/// Clippy's `cast_sign_loss` lint fires because it cannot statically prove
/// non-negativity from `clamp`, hence the `#[allow]` below.
#[inline]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
// Safety: clamp(0.0, MAX10) guarantees the value is in [0, 1023]; the
// intermediate `as u32` is non-negative and fits in u16.
fn rgba16_to_10bit_ycbcr_bt601(r: u16, g: u16, b: u16) -> [u16; 3] {
    const MAX10: f32 = 1023.0;
    // Scale factor from 16-bit (0-65535) to 10-bit (0-1023).
    const SCALE: f32 = MAX10 / 65535.0;
    const SHIFT: f32 = 512.0; // 2^(depth−1) = 2^9 = 512: unsigned chroma midpoint for 10-bit full-range YCbCr
                              // (ITU-R BT.601 unsigned representation; differs from 0.5×MAX10 = 511.5)
    const KR: f32 = 0.2990;
    const KG: f32 = 0.5870;
    const KB: f32 = 0.1140;

    let (rf, gf, bf) = (f32::from(r), f32::from(g), f32::from(b));

    let y = SCALE * (KR * rf + KG * gf + KB * bf);
    let cb = (SCALE * bf - y) * (0.5 / (1.0 - KB)) + SHIFT;
    let cr = (SCALE * rf - y) * (0.5 / (1.0 - KR)) + SHIFT;

    // Clamp before cast to avoid truncation/sign-loss from floating-point
    // rounding at the edges of the signal range.  We clamp to [0.0, MAX10]
    // then cast through u32 (always ≥ 0) to u16, satisfying the
    // `cast_sign_loss` lint without using `allow`.
    let clamp10 = |v: f32| v.round().clamp(0.0, MAX10) as u32 as u16;
    [clamp10(y), clamp10(cb), clamp10(cr)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::Pixels;
    use std::sync::Arc;

    fn solid_rgba8(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> RawImage {
        let pixel = [r, g, b, a];
        let pixels = pixel.repeat(width as usize * height as usize);
        RawImage {
            width,
            height,
            pixels: Pixels::Rgba8(Arc::from(pixels)),
        }
    }

    #[test]
    fn encode_and_validate_small_image() {
        let img = solid_rgba8(8, 8, 255, 0, 0, 255);
        let avif = encode_avif(&img, 80, 6, 80).expect("encode failed");
        assert!(avif.len() >= MIN_AVIF_BYTES);
        assert_eq!(&avif[4..8], b"ftyp");
    }

    #[test]
    fn validate_rejects_empty() {
        let err = validate_avif_output(&[], 4, 4).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn validate_rejects_too_short() {
        let err = validate_avif_output(&[0u8; 10], 4, 4).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn validate_rejects_missing_ftyp() {
        // 20 bytes but not an ftyp box
        let mut fake = vec![0u8; 20];
        fake[4..8].copy_from_slice(b"moov");
        let err = validate_avif_output(&fake, 4, 4).unwrap_err();
        assert!(matches!(err, Error::Encode(ref msg) if msg.contains("ftyp")));
    }

    #[test]
    fn validate_accepts_valid_ftyp() {
        let mut valid = vec![0u8; 24];
        // Set box size (big-endian u32) to 24 — the full buffer length.
        valid[0..4].copy_from_slice(&24u32.to_be_bytes());
        valid[4..8].copy_from_slice(b"ftyp");
        valid[8..12].copy_from_slice(b"avif");
        assert!(validate_avif_output(&valid, 4, 4).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_major_brand() {
        let mut fake = vec![0u8; 24];
        fake[0..4].copy_from_slice(&24u32.to_be_bytes());
        fake[4..8].copy_from_slice(b"ftyp");
        fake[8..12].copy_from_slice(b"mp41"); // Not an AVIF brand
        let err = validate_avif_output(&fake, 4, 4).unwrap_err();
        assert!(matches!(err, Error::Encode(ref msg) if msg.contains("major brand")));
    }

    #[test]
    fn validate_rejects_invalid_box_size() {
        let mut fake = vec![0u8; 24];
        fake[0..4].copy_from_slice(&5u32.to_be_bytes()); // Too small
        fake[4..8].copy_from_slice(b"ftyp");
        fake[8..12].copy_from_slice(b"avif");
        let err = validate_avif_output(&fake, 4, 4).unwrap_err();
        assert!(matches!(err, Error::Encode(ref msg) if msg.contains("box size")));
    }

    #[test]
    fn validate_accepts_avis_brand() {
        let mut valid = vec![0u8; 24];
        valid[0..4].copy_from_slice(&24u32.to_be_bytes());
        valid[4..8].copy_from_slice(b"ftyp");
        valid[8..12].copy_from_slice(b"avis");
        assert!(validate_avif_output(&valid, 4, 4).is_ok());
    }
}
