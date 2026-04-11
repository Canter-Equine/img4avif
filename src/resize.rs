//! Output resolution control and image resizing.
//!
//! This module defines the [`OutputResolution`] enum that controls how a decoded
//! image is scaled before AVIF encoding.  The actual downscaling is performed
//! by [`resize_raw_image`], which uses a Lanczos-3 filter for high quality.
//!
//! ## Behaviour
//!
//! - **Only downscales** — if the image is already at or below the target width
//!   it is returned unchanged.  This prevents quality loss from unnecessary
//!   upscaling.
//! - **Preserves aspect ratio** — the height is computed proportionally so the
//!   image is never cropped or stretched.
//! - **Both bit-depths** — 8-bit (`Rgba8`) and 16-bit (`Rgba16`) images are
//!   handled; 16-bit precision is preserved through the resize step.

use crate::decoder::{Pixels, RawImage};
use crate::logging::{img_debug, img_info};

/// Controls the output resolution applied before AVIF encoding.
///
/// Set this on [`Config`](crate::Config) via
/// [`Config::output_resolutions`](crate::Config::output_resolutions) to
/// produce one or more outputs at different sizes from a single decode pass.
///
/// # Downscale-only
///
/// When the decoded image is already **at or below** the target width the
/// pixels are passed through unchanged — `img2avif` never upscales.
///
/// # Aspect ratio
///
/// The height is always scaled proportionally so the image is never cropped
/// or distorted.
///
/// # Example
///
/// ```rust,no_run
/// use img2avif::{Config, Converter, OutputResolution};
///
/// # fn main() -> Result<(), img2avif::Error> {
/// // Produce only the 1080-wide variant.
/// let config = Config::default()
///     .output_resolutions(vec![OutputResolution::Width1080]);
/// let converter = Converter::new(config)?;
/// let avif_1080 = converter.convert(&std::fs::read("photo.jpg")?)?;
/// std::fs::write("photo_1080.avif", avif_1080)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputResolution {
    /// Preserve the original image dimensions (no resize).
    Original,
    /// Shrink so the width is at most **2560 pixels**, preserving the aspect
    /// ratio.  Images already ≤ 2560 px wide are passed through unchanged.
    Width2560,
    /// Shrink so the width is at most **1080 pixels**, preserving the aspect
    /// ratio.  Images already ≤ 1080 px wide are passed through unchanged.
    Width1080,
}

impl OutputResolution {
    /// Returns the maximum permitted width for this variant, or `None` for
    /// [`OutputResolution::Original`] (no limit).
    #[must_use]
    pub(crate) fn max_width(self) -> Option<u32> {
        match self {
            Self::Original => None,
            Self::Width2560 => Some(2560),
            Self::Width1080 => Some(1080),
        }
    }
}

/// Resize `raw` to fit within `resolution`, preserving the aspect ratio.
///
/// **Only downscales** — if the image width is already ≤ the target width the
/// input is returned unchanged without copying the pixel data.
///
/// Uses the Lanczos-3 filter for high-quality downsampling.  Both
/// [`Pixels::Rgba8`] and [`Pixels::Rgba16`] inputs are supported; 16-bit
/// precision is preserved throughout the resize step.
pub(crate) fn resize_raw_image(raw: RawImage, resolution: OutputResolution) -> RawImage {
    let Some(target_width) = resolution.max_width() else {
        // OutputResolution::Original — return the image unchanged.
        return raw;
    };

    let RawImage { width, height, pixels } = raw;

    if width <= target_width {
        img_debug!(
            "resize: {}×{} is already within {}px target — skipping",
            width, height, target_width
        );
        return RawImage { width, height, pixels };
    }

    let new_width = target_width;
    // Proportional height, rounded to nearest pixel.  Use u64 arithmetic to
    // avoid overflow when width × new_width exceeds u32::MAX.
    let new_height = u32::try_from(
        (u64::from(height) * u64::from(new_width) + u64::from(width) / 2) / u64::from(width),
    )
    .unwrap_or(1)
    .max(1);

    img_info!(
        "resize: {}×{} → {}×{} ({} target width, Lanczos3)",
        width, height, new_width, new_height, target_width
    );

    match pixels {
        Pixels::Rgba8(data) => {
            let buf = image::RgbaImage::from_raw(width, height, data)
                .expect("internal: RGBA8 buffer size mismatch");
            let resized = image::imageops::resize(
                &buf,
                new_width,
                new_height,
                image::imageops::FilterType::Lanczos3,
            );
            RawImage {
                width: new_width,
                height: new_height,
                pixels: Pixels::Rgba8(resized.into_raw()),
            }
        }
        Pixels::Rgba16(data) => {
            use image::{ImageBuffer, Rgba};
            let buf: ImageBuffer<Rgba<u16>, Vec<u16>> =
                ImageBuffer::from_raw(width, height, data)
                    .expect("internal: RGBA16 buffer size mismatch");
            let resized = image::imageops::resize(
                &buf,
                new_width,
                new_height,
                image::imageops::FilterType::Lanczos3,
            );
            RawImage {
                width: new_width,
                height: new_height,
                pixels: Pixels::Rgba16(resized.into_raw()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::Pixels;

    fn solid_rgba8(width: u32, height: u32) -> RawImage {
        let pixel = [255u8, 128, 64, 255];
        RawImage {
            width,
            height,
            pixels: Pixels::Rgba8(pixel.repeat(width as usize * height as usize)),
        }
    }

    #[test]
    fn original_is_unchanged() {
        let raw = solid_rgba8(4000, 3000);
        let out = resize_raw_image(raw, OutputResolution::Original);
        assert_eq!(out.width, 4000);
        assert_eq!(out.height, 3000);
    }

    #[test]
    fn no_upscale_when_already_small() {
        // A 640-wide image should not be upscaled to 2560 or 1080.
        let raw = solid_rgba8(640, 480);
        let out2560 = resize_raw_image(raw.clone(), OutputResolution::Width2560);
        assert_eq!(out2560.width, 640);
        assert_eq!(out2560.height, 480);

        let out1080 = resize_raw_image(raw, OutputResolution::Width1080);
        assert_eq!(out1080.width, 640);
        assert_eq!(out1080.height, 480);
    }

    #[test]
    fn downscales_to_2560() {
        let raw = solid_rgba8(5120, 2880); // 16:9 at 5K
        let out = resize_raw_image(raw, OutputResolution::Width2560);
        assert_eq!(out.width, 2560);
        assert_eq!(out.height, 1440); // 16:9 preserved
    }

    #[test]
    fn downscales_to_1080() {
        let raw = solid_rgba8(1920, 1080); // Full HD
        let out = resize_raw_image(raw, OutputResolution::Width1080);
        assert_eq!(out.width, 1080);
        // Height should be 1080 * 1080 / 1920 = 607.5 → 608
        assert_eq!(out.height, 608);
    }

    #[test]
    fn aspect_ratio_preserved_portrait() {
        // Portrait 2:3 at 4320×6480 (higher-res)
        let raw = solid_rgba8(4320, 6480);
        let out = resize_raw_image(raw, OutputResolution::Width2560);
        assert_eq!(out.width, 2560);
        // height = 6480 * 2560 / 4320 = 3840
        assert_eq!(out.height, 3840);
    }

    #[test]
    fn exact_target_width_is_not_resized() {
        let raw = solid_rgba8(2560, 1440);
        let out = resize_raw_image(raw, OutputResolution::Width2560);
        assert_eq!(out.width, 2560);
        assert_eq!(out.height, 1440);
    }

    #[test]
    fn output_resolution_max_width() {
        assert_eq!(OutputResolution::Original.max_width(), None);
        assert_eq!(OutputResolution::Width2560.max_width(), Some(2560));
        assert_eq!(OutputResolution::Width1080.max_width(), Some(1080));
    }
}
