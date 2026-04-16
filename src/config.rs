use crate::resize::OutputResolution;

/// Configuration for the AVIF converter.
/// # Defaults
///
/// | Field | Default |
/// |-------|---------|
/// | `quality` | `8` |
/// | `alpha_quality` | `8` |
/// | `speed` | `6` |
/// | `strip_exif` | `true` |
/// | `max_input_bytes` | 100 MiB |
/// | `max_pixels` | 16 384 × 16 384 (≈ 268 MP) |
/// | `memory_limit_bytes` | 512 MiB |
/// | `output_resolutions` | `[Original]` |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Encoding quality **1 – 10** (higher = better quality, larger file).
    /// Default: `8`.
    pub quality: u8,

    /// Alpha-channel encoding quality **1 – 10**.
    /// Default: `8`.
    pub alpha_quality: u8,

    /// Encoder speed **1 – 10** (higher = faster, slightly larger file).
    /// Default: `6`.
    pub speed: u8,

    /// Strip all EXIF, IPTC, and XMP metadata from the output.
    /// Set to `false` to preserve metadata; a warning is printed to `stderr`
    /// because metadata retention increases output size and Lambda cost.
    /// Default: `true`.
    pub strip_exif: bool,

    /// Maximum raw input size in bytes.
    /// Set to `u64::MAX` to disable.  Default: 100 MiB.
    pub max_input_bytes: u64,

    /// Maximum decoded pixel count (width × height).
    /// Set to `u64::MAX` to disable. Default: 16 384 × 16 384 (≈ 268 MP).
    pub max_pixels: u64,

    /// Peak RSS memory budget in bytes.
    /// Set to `u64::MAX` to disable.
    /// A 50 MP RGBA8 image occupies 200 MiB in the pixel buffer alone.
    /// Default: 512 MiB.
    pub memory_limit_bytes: u64,

    /// Which output resolution(s) to produce.
    ///
    /// - A **single entry** (the default) controls what [`crate::Converter::convert`]
    ///   produces.  Defaults to `[OutputResolution::Original]` (no resize).
    /// - **Multiple entries** are used by
    ///   [`Converter::convert_multi`](crate::Converter::convert_multi), which
    ///   decodes the image once and then encodes a separate AVIF for each
    ///   requested resolution.
    ///
    /// Images are only ever **downscaled** — if the source is already at or
    /// below the target width it is encoded at its original size.
    ///
    /// Default: `vec![OutputResolution::Original]`.
    pub output_resolutions: Vec<OutputResolution>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quality: 8,
            alpha_quality: 8,
            speed: 6,
            strip_exif: true,
            max_input_bytes: 100 * 1024 * 1024,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 512 * 1024 * 1024,
            output_resolutions: vec![OutputResolution::Original],
        }
    }
}

impl Config {
    /// Set the colour-channel encoding quality (1 – 10).
    #[must_use]
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q.clamp(1, 10);
        self
    }

    /// Set the alpha-channel encoding quality (1 – 10).
    #[must_use]
    pub fn alpha_quality(mut self, q: u8) -> Self {
        self.alpha_quality = q.clamp(1, 10);
        self
    }

    /// Set the encoder speed (1 – 10).
    /// Speed 10 is recommended for Lambda to minimise CPU-time billing.
    #[must_use]
    pub fn speed(mut self, s: u8) -> Self {
        self.speed = s.clamp(1, 10);
        self
    }

    /// Strip (`true`) or preserve (`false`) EXIF / metadata.
    #[must_use]
    pub fn strip_exif(mut self, strip: bool) -> Self {
        self.strip_exif = strip;
        self
    }

    /// Maximum input file size in bytes, checked before decompression.  Pass `u64::MAX` to disable.
    #[must_use]
    pub fn max_input_bytes(mut self, bytes: u64) -> Self {
        self.max_input_bytes = bytes;
        self
    }

    /// Maximum decoded pixel count.  Pass `u64::MAX` to disable.
    #[must_use]
    pub fn max_pixels(mut self, p: u64) -> Self {
        self.max_pixels = p;
        self
    }

    /// Peak RSS memory budget in bytes.  Pass `u64::MAX` to disable.
    #[must_use]
    pub fn memory_limit_bytes(mut self, bytes: u64) -> Self {
        self.memory_limit_bytes = bytes;
        self
    }

    /// Set which output resolution(s) to produce.
    ///
    /// Pass a `Vec` containing one or more [`OutputResolution`] variants:
    ///
    /// - A single entry controls what [`Converter::convert`](crate::Converter::convert)
    ///   produces.
    /// - Multiple entries are consumed by
    ///   [`Converter::convert_multi`](crate::Converter::convert_multi), which
    ///   decodes the image **once** and encodes a separate AVIF file for each
    ///   requested resolution.
    ///
    /// If an empty `Vec` is supplied, [`crate::Converter::convert`] falls back to
    /// [`OutputResolution::Original`] (no resize).
    ///
    /// # Example
    ///
    /// ```rust
    /// use img4avif::{Config, OutputResolution};
    ///
    /// // Produce all three resolutions in one convert_multi call.
    /// let config = Config::default().output_resolutions(vec![
    ///     OutputResolution::Original,
    ///     OutputResolution::Width2560,
    ///     OutputResolution::Width1080,
    /// ]);
    /// ```
    #[must_use]
    pub fn output_resolutions(mut self, resolutions: Vec<OutputResolution>) -> Self {
        self.output_resolutions = resolutions;
        self
    }

    /// Preset tuned for minimum Lambda cost: fastest encoder speed (10),
    /// quality 8, EXIF stripped, 50 MiB input cap, 512 MiB memory budget.
    #[must_use]
    pub fn lambda_cost_optimized() -> Self {
        Self {
            quality: 8,
            alpha_quality: 8,
            speed: 10,
            strip_exif: true,
            max_input_bytes: 50 * 1024 * 1024,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 512 * 1024 * 1024,
            output_resolutions: vec![OutputResolution::Original],
        }
    }
}
