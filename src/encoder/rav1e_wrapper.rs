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

use crate::decoder::{Pixels, RawImage};
use crate::Error;

/// Encode a [`RawImage`] as AVIF.
///
/// - For **8-bit** inputs (`Pixels::Rgba8`): calls `ravif::Encoder::encode_rgba`.
///   `rav1e` auto-selects 10-bit encoding internally for better quality.
/// - For **16-bit** inputs (`Pixels::Rgba16`): calls
///   `ravif::Encoder::encode_raw_planes_10_bit` so the full 10-bit precision
///   is preserved rather than being silently discarded.
///
/// `quality` must be in **1 – 100** (higher = better).
/// `speed` must be in **1 – 10** (higher = faster).
/// `alpha_quality` must be in **1 – 100**; pass the same value as `quality`
/// for uniform quality, or a higher value (e.g. 95) to keep the alpha channel
/// visually lossless.
///
/// # Errors
///
/// Returns [`Error::Encode`] if `rav1e` fails to produce a valid bitstream.
pub fn encode_avif(
    image: &RawImage,
    quality: u8,
    speed: u8,
    alpha_quality: u8,
) -> Result<Vec<u8>, Error> {
    match &image.pixels {
        Pixels::Rgba8(bytes) => encode_8bit(image.width, image.height, bytes, quality, speed, alpha_quality),
        Pixels::Rgba16(samples) => {
            encode_16bit(image.width, image.height, samples, quality, speed, alpha_quality)
        }
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
    use rgb::RGBA8;

    let rgba: Vec<RGBA8> = pixels
        .chunks_exact(4)
        .map(|c| RGBA8::new(c[0], c[1], c[2], c[3]))
        .collect();

    let img = Img::new(rgba.as_slice(), width as usize, height as usize);

    let EncodedImage { avif_file, .. } = Encoder::new()
        .with_quality(f32::from(quality.clamp(1, 100)))
        .with_alpha_quality(f32::from(alpha_quality.clamp(1, 100)))
        .with_speed(speed.clamp(1, 10))
        .encode_rgba(img)
        .map_err(|e| Error::Encode(e.to_string()))?;

    Ok(avif_file)
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

    let EncodedImage { avif_file, .. } = Encoder::new()
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
        .map_err(|e| Error::Encode(e.to_string()))?;

    Ok(avif_file)
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
    const SHIFT: f32 = 511.0; // 0.5 * MAX10, rounded
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
