//! AVIF encoding via the `ravif` / `rav1e` pure-Rust AV1 encoder.
//!
//! This module wraps [`ravif::Encoder`] and converts between our internal
//! [`RawImage`] representation and the types expected by `ravif`.

use crate::{decoder::RawImage, Error};

/// Encode a [`RawImage`] as AVIF using the `rav1e` encoder.
///
/// `quality` must be in **1 – 100** (higher = better).
/// `speed` must be in **1 – 10** (higher = faster).
///
/// # Errors
///
/// Returns [`Error::Encode`] if `rav1e` fails to produce a valid bitstream.
pub fn encode_avif(image: &RawImage, quality: u8, speed: u8) -> Result<Vec<u8>, Error> {
    use ravif::{Encoder, Img};
    use rgb::RGBA8;

    // Convert flat RGBA bytes to a slice of `rgb::RGBA8` structs.
    let pixels: Vec<RGBA8> = image
        .pixels
        .chunks_exact(4)
        .map(|c| RGBA8::new(c[0], c[1], c[2], c[3]))
        .collect();

    let img = Img::new(
        pixels.as_slice(),
        image.width as usize,
        image.height as usize,
    );

    // ravif requires quality in [1.0, 100.0] and speed in [1, 10].
    let ravif_quality = f32::from(quality.clamp(1, 100));
    let ravif_speed = speed.clamp(1, 10);

    let result = Encoder::new()
        .with_quality(ravif_quality)
        .with_speed(ravif_speed)
        .encode_rgba(img)
        .map_err(|e| Error::Encode(e.to_string()))?;

    Ok(result.avif_file.clone())
}
