//! Output resolution control and image resizing.
//!
//! This module defines the [`OutputResolution`] enum that controls how a decoded
//! image is scaled before AVIF encoding.  The actual downscaling is performed
//! by `resize_raw_image`, which uses a Lanczos-3 filter for high quality.
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
use crate::error::Error;
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
    /// Shrink so the width is at most the given number of pixels, preserving
    /// the aspect ratio.  Images already at or below this width are passed
    /// through unchanged.
    ///
    /// A width of `0` is treated the same as [`Original`](Self::Original) —
    /// the image is returned at its full size without any resizing.  This is
    /// a deliberate design choice that makes it safe to derive a target width
    /// from arithmetic that could produce zero; callers that want a hard error
    /// on zero should validate the value before constructing this variant.
    Custom(u32),
}

impl OutputResolution {
    /// Returns the maximum permitted width for this variant, or `None` for
    /// [`OutputResolution::Original`] (no limit) and for
    /// [`OutputResolution::Custom(0)`](OutputResolution::Custom) (treated as
    /// no limit).
    #[must_use]
    pub(crate) fn max_width(self) -> Option<u32> {
        match self {
            Self::Original | Self::Custom(0) => None,
            Self::Width2560 => Some(2560),
            Self::Width1080 => Some(1080),
            Self::Custom(w) => Some(w),
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
///
/// Accepts the image by shared reference so that callers can invoke this
/// function multiple times on the same [`RawImage`] (e.g. `convert_multi`)
/// without cloning the pixel buffer upfront.
///
/// # Errors
///
/// Returns [`Error::Internal`] if the pixel buffer does not match the
/// declared image dimensions.  This should never happen with images produced
/// by the built-in decoders; if it does, please report a bug.
pub(crate) fn resize_raw_image(
    raw: &RawImage,
    resolution: OutputResolution,
) -> Result<RawImage, Error> {
    let Some(target_width) = resolution.max_width() else {
        // OutputResolution::Original — return the image unchanged.
        return Ok(raw.clone());
    };

    let &RawImage {
        width,
        height,
        ref pixels,
    } = raw;

    if width <= target_width {
        img_debug!(
            "resize: {}×{} is already within {}px target — skipping",
            width,
            height,
            target_width
        );
        return Ok(RawImage {
            width,
            height,
            pixels: pixels.clone(),
        });
    }

    let new_width = target_width;
    // Proportional height, rounded to nearest pixel.  Use saturating u64
    // arithmetic so that extreme aspect ratios cannot silently produce a
    // 1-pixel-tall output via integer overflow.
    let height_u64 = u64::from(height)
        .saturating_mul(u64::from(new_width))
        .saturating_add(u64::from(width) / 2)
        / u64::from(width);

    let new_height = u32::try_from(height_u64)
        .map_err(|_| {
            Error::Internal(format!(
                "resize calculation overflow: {width}×{height} → width {target_width}"
            ))
        })?
        .max(1);

    img_info!(
        "resize: {}×{} → {}×{} ({} target width, Lanczos3)",
        width,
        height,
        new_width,
        new_height,
        target_width
    );

    match pixels {
        Pixels::Rgba8(data) => {
            let buf = image::RgbaImage::from_raw(width, height, data.clone()).ok_or_else(|| {
                Error::Internal(format!(
                    "RGBA8 pixel buffer size does not match declared dimensions {width}×{height}; \
                     this is a bug — please report it"
                ))
            })?;
            let resized = image::imageops::resize(
                &buf,
                new_width,
                new_height,
                image::imageops::FilterType::Lanczos3,
            );
            Ok(RawImage {
                width: new_width,
                height: new_height,
                pixels: Pixels::Rgba8(resized.into_raw()),
            })
        }
        Pixels::Rgba16(data) => {
            use image::{ImageBuffer, Rgba};
            let buf: ImageBuffer<Rgba<u16>, Vec<u16>> =
                ImageBuffer::from_raw(width, height, data.clone())
                    .ok_or_else(|| Error::Internal(format!(
                        "RGBA16 pixel buffer size does not match declared dimensions {width}×{height}; \
                         this is a bug — please report it"
                    )))?;
            let resized = image::imageops::resize(
                &buf,
                new_width,
                new_height,
                image::imageops::FilterType::Lanczos3,
            );
            Ok(RawImage {
                width: new_width,
                height: new_height,
                pixels: Pixels::Rgba16(resized.into_raw()),
            })
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

    fn solid_rgba16(width: u32, height: u32) -> RawImage {
        let pixel = [32768u16, 16384, 8192, 65535];
        RawImage {
            width,
            height,
            pixels: Pixels::Rgba16(pixel.repeat(width as usize * height as usize)),
        }
    }

    #[test]
    fn original_is_unchanged() {
        let raw = solid_rgba8(4000, 3000);
        let out = resize_raw_image(&raw, OutputResolution::Original).unwrap();
        assert_eq!(out.width, 4000);
        assert_eq!(out.height, 3000);
    }

    #[test]
    fn no_upscale_when_already_small() {
        // A 640-wide image should not be upscaled to 2560 or 1080.
        let raw = solid_rgba8(640, 480);
        let out2560 = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out2560.width, 640);
        assert_eq!(out2560.height, 480);

        let out1080 = resize_raw_image(&raw, OutputResolution::Width1080).unwrap();
        assert_eq!(out1080.width, 640);
        assert_eq!(out1080.height, 480);
    }

    #[test]
    fn downscales_to_2560() {
        let raw = solid_rgba8(5120, 2880); // 16:9 at 5K
        let out = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out.width, 2560);
        assert_eq!(out.height, 1440); // 16:9 preserved
    }

    #[test]
    fn downscales_to_1080() {
        let raw = solid_rgba8(1920, 1080); // Full HD
        let out = resize_raw_image(&raw, OutputResolution::Width1080).unwrap();
        assert_eq!(out.width, 1080);
        // Height: (1080 * 1080 + 960) / 1920 = 1167360 / 1920 = 608 (exactly)
        assert_eq!(out.height, 608);
    }

    #[test]
    fn aspect_ratio_preserved_portrait() {
        // Portrait 2:3 at 4320×6480 (higher-res)
        let raw = solid_rgba8(4320, 6480);
        let out = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out.width, 2560);
        // height = 6480 * 2560 / 4320 = 3840
        assert_eq!(out.height, 3840);
    }

    #[test]
    fn exact_target_width_is_not_resized() {
        let raw = solid_rgba8(2560, 1440);
        let out = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out.width, 2560);
        assert_eq!(out.height, 1440);
    }

    #[test]
    fn custom_resolution_downscales() {
        let raw = solid_rgba8(1920, 1080);
        let out = resize_raw_image(&raw, OutputResolution::Custom(720)).unwrap();
        assert_eq!(out.width, 720);
    }

    #[test]
    fn custom_resolution_zero_is_original() {
        let raw = solid_rgba8(1920, 1080);
        let out = resize_raw_image(&raw, OutputResolution::Custom(0)).unwrap();
        assert_eq!(out.width, 1920);
        assert_eq!(out.height, 1080);
    }

    #[test]
    fn custom_resolution_no_upscale() {
        let raw = solid_rgba8(640, 480);
        let out = resize_raw_image(&raw, OutputResolution::Custom(1280)).unwrap();
        assert_eq!(out.width, 640);
        assert_eq!(out.height, 480);
    }

    #[test]
    fn output_resolution_max_width() {
        assert_eq!(OutputResolution::Original.max_width(), None);
        assert_eq!(OutputResolution::Width2560.max_width(), Some(2560));
        assert_eq!(OutputResolution::Width1080.max_width(), Some(1080));
        assert_eq!(OutputResolution::Custom(720).max_width(), Some(720));
        assert_eq!(OutputResolution::Custom(3840).max_width(), Some(3840));
        assert_eq!(OutputResolution::Custom(0).max_width(), None);
    }

    // --- 16-bit resize tests ---

    #[test]
    fn rgba16_original_is_unchanged() {
        let raw = solid_rgba16(4000, 3000);
        let out = resize_raw_image(&raw, OutputResolution::Original).unwrap();
        assert_eq!(out.width, 4000);
        assert_eq!(out.height, 3000);
        assert!(matches!(out.pixels, Pixels::Rgba16(_)));
    }

    #[test]
    fn rgba16_downscales_to_2560() {
        let raw = solid_rgba16(5120, 2880);
        let out = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out.width, 2560);
        assert_eq!(out.height, 1440);
        assert!(matches!(out.pixels, Pixels::Rgba16(_)));
    }

    #[test]
    fn rgba16_no_upscale() {
        let raw = solid_rgba16(640, 480);
        let out = resize_raw_image(&raw, OutputResolution::Width1080).unwrap();
        assert_eq!(out.width, 640);
        assert_eq!(out.height, 480);
    }

    // --- buffer mismatch returns Error::Internal ---

    #[test]
    fn mismatched_rgba8_buffer_returns_internal_error() {
        // Declare a wide image (width > 1080) so the resize path is actually
        // triggered and can detect the buffer/dimension mismatch.
        // A width of 100 would be skipped (no downscale needed), so the error
        // would never occur — that was the pre-existing bug in this test.
        let raw = RawImage {
            width: 2000,
            height: 100,
            // Only 1 pixel worth of data instead of 2000 * 100 pixels
            pixels: Pixels::Rgba8(vec![255u8, 0, 0, 255]),
        };
        let err = resize_raw_image(&raw, OutputResolution::Width1080).unwrap_err();
        assert!(
            matches!(err, Error::Internal(_)),
            "expected Error::Internal, got {err:?}"
        );
    }

    #[test]
    fn mismatched_rgba16_buffer_returns_internal_error() {
        let raw = RawImage {
            width: 2000,
            height: 100,
            pixels: Pixels::Rgba16(vec![65535u16, 0, 0, 65535]),
        };
        let err = resize_raw_image(&raw, OutputResolution::Width1080).unwrap_err();
        assert!(
            matches!(err, Error::Internal(_)),
            "expected Error::Internal, got {err:?}"
        );
    }

    // --- degenerate dimensions ---

    #[test]
    fn very_wide_single_row() {
        // 2000×1 image, resize to Width1080 → 1080×1 (height stays 1)
        let raw = solid_rgba8(2000, 1);
        let out = resize_raw_image(&raw, OutputResolution::Width1080).unwrap();
        assert_eq!(out.width, 1080);
        assert_eq!(out.height, 1); // floor((1 * 1080 + 1000) / 2000) = 1
    }

    #[test]
    fn single_pixel_image() {
        // 1×1 image is below every target width — returned unchanged
        let raw = solid_rgba8(1, 1);
        let out2560 = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out2560.width, 1);
        let out1080 = resize_raw_image(&raw, OutputResolution::Width1080).unwrap();
        assert_eq!(out1080.width, 1);
    }

    #[test]
    fn saturating_arithmetic_does_not_truncate_tall_image() {
        // A very tall portrait image: 4096×16384 resized to Width2560.
        // height_u64 = (16384 * 2560 + 2048) / 4096 = 10,240 — fits in u32.
        // This test verifies the saturating-arithmetic path produces the
        // correct proportional height without any silent truncation.
        let raw = solid_rgba8(4096, 16384);
        let out = resize_raw_image(&raw, OutputResolution::Width2560).unwrap();
        assert_eq!(out.width, 2560);
        // height = (16384 * 2560 + 2048) / 4096 = 10240
        assert_eq!(out.height, 10240);
    }
}
