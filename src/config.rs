/// Configuration for the AVIF converter.
///
/// Setter methods consume `self` and return the modified value (builder pattern):
///
/// ```rust
/// use img2avif::Config;
///
/// let config = Config::default()
///     .quality(85)
///     .speed(6)
///     .strip_exif(true);
/// ```
///
/// # Defaults
///
/// | Field | Default |
/// |-------|---------|
/// | `quality` | `80` |
/// | `speed` | `6` |
/// | `strip_exif` | `true` |
/// | `max_input_bytes` | 100 MiB |
/// | `max_pixels` | 16 384 × 16 384 (≈ 268 MP) |
/// | `memory_limit_bytes` | 512 MiB |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Encoding quality **1 – 100** (higher = better quality, larger file).
    /// Clamped to the valid range on assignment.  Default: `80`.
    pub quality: u8,

    /// Encoder speed **1 – 10** (higher = faster, slightly larger file).
    /// Clamped to the valid range on assignment.  Default: `6`.
    pub speed: u8,

    /// Strip all EXIF, IPTC, and XMP metadata from the output.
    ///
    /// Set to `false` to preserve metadata; a warning is printed to `stderr`
    /// because metadata retention increases output size and Lambda cost.
    /// Default: `true`.
    pub strip_exif: bool,

    /// Maximum raw input size in bytes.
    ///
    /// Checked before any decompression so that oversized uploads are rejected
    /// immediately.  Set to `u64::MAX` to disable.  Default: 100 MiB.
    pub max_input_bytes: u64,

    /// Maximum decoded pixel count (width × height).
    ///
    /// The decoder allocation budget is derived from this value, which prevents
    /// decompression-bomb attacks.  Set to `u64::MAX` to disable.
    /// Default: 16 384 × 16 384 (≈ 268 MP).
    pub max_pixels: u64,

    /// Peak RSS memory budget in bytes.
    ///
    /// Checked before and after decoding on Linux (via `/proc/self/status`)
    /// and macOS.  Set to `u64::MAX` to disable.
    ///
    /// A 50 MP RGBA8 image occupies 200 MiB in the pixel buffer alone, so the
    /// default is sized to accommodate that with headroom.  Default: 512 MiB.
    pub memory_limit_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quality: 80,
            speed: 6,
            strip_exif: true,
            max_input_bytes: 100 * 1024 * 1024,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Config {
    /// Set the encoding quality (1 – 100). Values are clamped to this range.
    #[must_use]
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q.clamp(1, 100);
        self
    }

    /// Set the encoder speed (1 – 10). Values are clamped to this range.
    ///
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

    /// Preset tuned for minimum Lambda cost: fastest encoder speed (10),
    /// quality 75, EXIF stripped, 50 MiB input cap, 512 MiB memory budget.
    #[must_use]
    pub fn lambda_cost_optimized() -> Self {
        Self {
            quality: 75,
            speed: 10,
            strip_exif: true,
            max_input_bytes: 50 * 1024 * 1024,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 512 * 1024 * 1024,
        }
    }
}
