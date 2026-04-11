//! # img2avif
//!
//! A high-performance, memory-safe Rust library that converts images from
//! **JPEG, PNG, and WebP** into the **AVIF** format using the pure-Rust
//! `rav1e` AV1 encoder.
//!
//! Designed for cost-sensitive, high-volume **serverless workloads** (AWS
//! Lambda / Linux `x86_64` & `aarch64`) with built-in safeguards against memory
//! exhaustion and malformed-input attacks.
//!
//! ## Feature flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `heic-experimental` | off | HEIC/HEIF decoding via `libheif` (**requires C library**) |
//! | `raw-experimental`  | off | Camera RAW decoding via `rawloader` (pure Rust, unstable API) |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use img2avif::{Config, Converter};
//!
//! # fn main() -> Result<(), img2avif::Error> {
//! let jpeg_bytes = std::fs::read("photo.jpg")?;
//!
//! let config = Config::default()
//!     .quality(85)
//!     .speed(6)
//!     .strip_exif(true); // already the default
//!
//! let converter = Converter::new(config)?;
//! let avif_bytes = converter.convert(&jpeg_bytes)?;
//!
//! std::fs::write("photo.avif", &avif_bytes)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## EXIF / metadata handling
//!
//! By default **all EXIF, IPTC, and XMP metadata is stripped** from the
//! output to minimise file size (important for cost-sensitive Lambda
//! workloads).  Pass [`Config::strip_exif`]`(false)` to preserve metadata —
//! but be aware this increases output size and per-invocation cost.
//!
//! ## Memory safety
//!
//! A [`MemoryGuard`] is checked before and after decoding.  If peak RSS
//! exceeds [`Config::memory_limit_bytes`] (default 150 MB) the conversion
//! is aborted and [`Error::MemoryExceeded`] is returned immediately.
//! Memory checking is available on Linux and macOS; on other platforms the
//! check is a no-op (fail-open).
//!
//! ## No unsafe code
//!
//! `img2avif` does not contain any `unsafe` code.  All dependencies that
//! use `unsafe` (e.g. `rav1e`) are vendored through well-audited crates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

/// Configuration for the AVIF converter — see [`Config`].
pub mod config;
/// Error types — see [`Error`].
pub mod error;
/// Runtime memory guard — see [`MemoryGuard`].
pub mod memory_guard;
/// EXIF / metadata stripping utilities.
pub mod metadata;

pub(crate) mod decoder;
pub(crate) mod encoder;

pub use config::Config;
pub use error::Error;
pub use memory_guard::MemoryGuard;

/// The main conversion entry-point.
///
/// Instantiate once — ideally outside the hot path — then call
/// [`Converter::convert`] for each image.
///
/// # Example
///
/// ```rust,no_run
/// use img2avif::{Config, Converter};
///
/// # fn main() -> Result<(), img2avif::Error> {
/// let converter = Converter::new(Config::default())?;
/// let avif = converter.convert(&std::fs::read("input.png")?)?;
/// std::fs::write("output.avif", avif)?;
/// # Ok(())
/// # }
/// ```
#[must_use = "call `convert` to perform the conversion"]
pub struct Converter {
    config: Config,
}

impl Converter {
    /// Create a new [`Converter`] from the given [`Config`].
    ///
    /// # Errors
    ///
    /// Currently infallible; returns `Ok` for all valid configs.
    /// Future versions may validate config fields.
    pub fn new(config: Config) -> Result<Self, Error> {
        Ok(Self { config })
    }

    /// Convert raw image bytes (JPEG, PNG, or WebP) to AVIF.
    ///
    /// Returns the encoded AVIF file as a `Vec<u8>`.
    ///
    /// # Errors
    ///
    /// | Variant | Cause |
    /// |---------|-------|
    /// | [`Error::Decode`] | Input bytes could not be decoded |
    /// | [`Error::InputTooLarge`] | Pixel count exceeds [`Config::max_pixels`] |
    /// | [`Error::MemoryExceeded`] | Peak RSS exceeded [`Config::memory_limit_bytes`] |
    /// | [`Error::Encode`] | AVIF encoding failed |
    /// | [`Error::UnsupportedFormat`] | Format not enabled at compile time |
    pub fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if !self.config.strip_exif {
            // Use eprintln so callers on Lambda can see it in CloudWatch.
            eprintln!(
                "img2avif: preserve_metadata is enabled — \
                 metadata retention increases output size and Lambda cost"
            );
        }

        let guard = MemoryGuard::new(self.config.memory_limit_bytes);

        // Baseline memory check before any allocation.
        guard.check()?;

        // Strip or preserve metadata on the raw input bytes so the decoder
        // never processes potentially malformed metadata chunks.
        let processed: Vec<u8> = if self.config.strip_exif {
            metadata::strip_metadata(input)
        } else {
            input.to_vec()
        };

        // Decode to raw RGBA8 pixels.
        let raw = decoder::decode(&processed)?;

        // Enforce pixel-count limit before the post-decode RSS check.
        let pixel_count = u64::from(raw.width) * u64::from(raw.height);
        if pixel_count > self.config.max_pixels {
            return Err(Error::InputTooLarge {
                width: raw.width,
                height: raw.height,
                max_pixels: self.config.max_pixels,
            });
        }

        // Post-decode memory check: decode buffer is now live.
        guard.check()?;

        // Encode to AVIF.
        let avif = encoder::encode_avif(&raw, self.config.quality, self.config.speed)?;

        Ok(avif)
    }

    /// Return the [`Config`] this converter was created with.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::new(width, height);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn round_trip_png() {
        let png = make_minimal_png(4, 4);
        let converter = Converter::new(Config::default()).unwrap();
        let avif = converter.convert(&png).expect("conversion failed");
        assert!(!avif.is_empty());
    }

    #[test]
    fn rejects_input_too_large() {
        let png = make_minimal_png(4, 4);
        let config = Config::default().max_pixels(1);
        let converter = Converter::new(config).unwrap();
        let err = converter.convert(&png).unwrap_err();
        assert!(matches!(err, Error::InputTooLarge { .. }));
    }

    #[test]
    fn rejects_garbage_input() {
        let garbage = b"this is not an image";
        let converter = Converter::new(Config::default()).unwrap();
        let err = converter.convert(garbage).unwrap_err();
        assert!(matches!(
            err,
            Error::Decode(_) | Error::UnsupportedFormat(_)
        ));
    }

    #[test]
    fn config_builder_clamps_values() {
        let cfg = Config::default().quality(200).speed(99);
        assert_eq!(cfg.quality, 100);
        assert_eq!(cfg.speed, 10);
        let cfg_low = Config::default().quality(0);
        assert_eq!(cfg_low.quality, 1);
    }

    #[test]
    fn config_accessor() {
        let cfg = Config::default().quality(42);
        let converter = Converter::new(cfg).unwrap();
        assert_eq!(converter.config().quality, 42);
    }
}
