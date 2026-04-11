//! # img2avif
//!
//! Converts **JPEG, PNG, WebP, and HEIC/HEIF** images to AVIF using the
//! pure-Rust `rav1e` AV1 encoder.  Designed for serverless workloads (AWS
//! Lambda `x86_64` / `aarch64`) with built-in guards against memory
//! exhaustion and malformed input.
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
//!     .strip_exif(true); // default
//!
//! let converter = Converter::new(config)?;
//! let avif_bytes = converter.convert(&jpeg_bytes)?;
//!
//! std::fs::write("photo.avif", &avif_bytes)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Supported input formats
//!
//! | Format | File extensions | Feature flag | Notes |
//! |--------|----------------|-------------|-------|
//! | JPEG | `.jpg`, `.jpeg` | *(always on)* | 8-bit, greyscale or YCbCr |
//! | PNG | `.png` | *(always on)* | 8-bit and 16-bit (HDR10) |
//! | WebP | `.webp` | *(always on)* | lossy and lossless |
//! | HEIC / HEIF | `.heic`, `.heif` | `heic-experimental` | Requires `libheif` C library |
//!
//! ## HDR10
//!
//! 16-bit PNG files commonly used to distribute HDR10 content are accepted
//! natively: the `image` crate scales each channel from 16 to 8 bits before
//! encoding.  The AVIF output is an SDR file; full HDR10 (BT.2020 + PQ) AVIF
//! output requires a future encoder upgrade.
//!
//! HEIC files that carry HDR10 colour profiles are decoded by `libheif` when
//! the `heic-experimental` feature is enabled.
//!
//! ## Security model
//!
//! - **Input-size cap** ([`Config::max_input_bytes`], default 100 MiB) —
//!   rejected before any bytes are decompressed.
//! - **Decompression-bomb protection** ([`Config::max_pixels`]) — the decoder
//!   allocation budget is derived from `max_pixels * 4 + 64 MiB`; an image
//!   that claims huge dimensions is rejected before the pixel buffer lands in
//!   RAM.
//! - **RSS guard** ([`Config::memory_limit_bytes`], default 512 MiB) — checked
//!   before and after decode; breaches return [`Error::MemoryExceeded`].
//! - **No unsafe code** — enforced by `#![forbid(unsafe_code)]`.
//!
//! ## Feature flags
//!
//! | Flag | Default | Notes |
//! |------|---------|-------|
//! | `heic-experimental` | off | HEIC/HEIF support via the `libheif` C library. Linking `libheif` makes the binary LGPL-encumbered. |
//! | `raw-experimental`  | off | Pure Rust RAW camera format support via `rawloader`. Unstable API. |

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

/// Configuration — see [`Config`].
pub mod config;
/// Error types — see [`Error`].
pub mod error;
/// RSS memory guard — see [`MemoryGuard`].
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

    /// Convert raw image bytes to AVIF.
    ///
    /// The input format is detected automatically from magic bytes; the
    /// following formats are supported:
    ///
    /// | Format | Always available? |
    /// |--------|------------------|
    /// | JPEG / JPG | ✓ |
    /// | PNG (8-bit and 16-bit / HDR10) | ✓ |
    /// | WebP | ✓ |
    /// | HEIC / HEIF | `heic-experimental` feature only |
    ///
    /// Returns the encoded AVIF file as a `Vec<u8>`.
    ///
    /// # Errors
    ///
    /// | Variant | Cause |
    /// |---------|-------|
    /// | [`Error::Decode`] | Input could not be decoded (includes oversized input) |
    /// | [`Error::InputTooLarge`] | Pixel count exceeds [`Config::max_pixels`] |
    /// | [`Error::MemoryExceeded`] | Peak RSS exceeded [`Config::memory_limit_bytes`] |
    /// | [`Error::Encode`] | AVIF encoding failed |
    /// | [`Error::UnsupportedFormat`] | Format not supported in this build (e.g., HEIC without `heic-experimental`) |
    pub fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if !self.config.strip_exif {
            eprintln!(
                "img2avif: preserve_metadata is enabled — \
                 metadata retention increases output size and Lambda cost"
            );
        }

        // Reject oversized uploads before touching any bytes.
        if input.len() as u64 > self.config.max_input_bytes {
            return Err(Error::Decode(format!(
                "input too large: {} bytes exceeds the {}-byte limit",
                input.len(),
                self.config.max_input_bytes,
            )));
        }

        let guard = MemoryGuard::new(self.config.memory_limit_bytes);
        guard.check()?;

        // Strip metadata before decode so the image library never sees
        // potentially malformed APP / ancillary chunks.
        let processed: Vec<u8> = if self.config.strip_exif {
            metadata::strip_metadata(input)
        } else {
            input.to_vec()
        };

        // Decode to RGBA8.  The decoder enforces max_pixels internally to
        // prevent decompression-bomb attacks.
        let raw = decoder::decode(&processed, self.config.max_pixels)?;

        // Post-decode RSS check: the pixel buffer is now live.
        guard.check()?;

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
