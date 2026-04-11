/// Configuration for the AVIF converter.
///
/// All setter methods consume `self` and return the modified value, enabling
/// a fluent builder pattern:
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
/// | Field | Default | Notes |
/// |-------|---------|-------|
/// | `quality` | `80` | Good balance of size and fidelity |
/// | `speed` | `6` | Balanced encode time vs. compression |
/// | `strip_exif` | `true` | Reduces output size; preferred for Lambda |
/// | `max_pixels` | `268_435_456` | 16384 × 16384 |
/// | `memory_limit_bytes` | `157_286_400` | 150 MB |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Encoding quality in the range **1 – 100** (higher = better quality,
    /// larger file). Values above 100 are clamped to 100; values below 1 are
    /// clamped to 1.
    ///
    /// Default: `80`.
    pub quality: u8,

    /// Encoder speed in the range **1 – 10** (higher = faster encoding,
    /// slightly larger file). Values outside 1 – 10 are clamped.
    ///
    /// Default: `6`.
    pub speed: u8,

    /// When `true` (default) all EXIF, IPTC, and XMP metadata is stripped
    /// from the output.
    ///
    /// Setting this to `false` preserves metadata but **increases output size
    /// and Lambda invocation cost**.  A warning is printed to `stderr` at
    /// conversion time when metadata preservation is active.
    ///
    /// Default: `true`.
    pub strip_exif: bool,

    /// Maximum allowed pixel count (width × height).
    ///
    /// Images exceeding this limit are rejected with [`Error::InputTooLarge`]
    /// before any expensive decoding takes place.
    ///
    /// Default: `16_384 × 16_384 = 268_435_456` pixels (≈ 268 MP).
    ///
    /// [`Error::InputTooLarge`]: crate::Error::InputTooLarge
    pub max_pixels: u64,

    /// Peak RSS memory budget in **bytes**.
    ///
    /// If the process RSS exceeds this limit during conversion,
    /// [`Error::MemoryExceeded`] is returned immediately.  Set to `u64::MAX`
    /// to disable the check.
    ///
    /// Memory monitoring is available on Linux and macOS.  On other platforms
    /// the check is silently skipped (fail-open).
    ///
    /// Default: `150 × 1024 × 1024` (150 MB).
    ///
    /// [`Error::MemoryExceeded`]: crate::Error::MemoryExceeded
    pub memory_limit_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quality: 80,
            speed: 6,
            strip_exif: true,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 150 * 1024 * 1024,
        }
    }
}

impl Config {
    /// Set the encoding quality (1 – 100). Values are clamped to this range.
    ///
    /// Higher values produce better fidelity at the cost of larger files.
    #[must_use]
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q.clamp(1, 100);
        self
    }

    /// Set the encoder speed (1 – 10). Values are clamped to this range.
    ///
    /// Higher values encode faster but may produce slightly larger files.
    /// Speed 10 is recommended for Lambda to minimise CPU cost.
    #[must_use]
    pub fn speed(mut self, s: u8) -> Self {
        self.speed = s.clamp(1, 10);
        self
    }

    /// Control EXIF / metadata stripping.
    ///
    /// `true` (default) strips all EXIF, IPTC, and XMP metadata.
    /// `false` preserves metadata — see the `strip_exif` field docs for cost
    /// implications.
    #[must_use]
    pub fn strip_exif(mut self, strip: bool) -> Self {
        self.strip_exif = strip;
        self
    }

    /// Override the maximum pixel count (width × height).
    ///
    /// Pass `u64::MAX` to disable dimension validation.
    #[must_use]
    pub fn max_pixels(mut self, p: u64) -> Self {
        self.max_pixels = p;
        self
    }

    /// Override the peak RSS memory budget in bytes.
    ///
    /// Pass `u64::MAX` to disable the memory guard.
    #[must_use]
    pub fn memory_limit_bytes(mut self, bytes: u64) -> Self {
        self.memory_limit_bytes = bytes;
        self
    }

    /// Return a [`Config`] preset tuned for minimal Lambda cost:
    /// fast encoder, aggressive EXIF stripping, tight memory budget.
    ///
    /// Quality is reduced slightly (75) to further reduce output bandwidth.
    #[must_use]
    pub fn lambda_cost_optimized() -> Self {
        Self {
            quality: 75,
            speed: 10,
            strip_exif: true,
            max_pixels: 16_384 * 16_384,
            memory_limit_bytes: 150 * 1024 * 1024,
        }
    }
}
