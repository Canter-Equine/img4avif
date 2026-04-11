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
//! | Format | Extensions | Feature flag | AVIF output bit-depth |
//! |--------|-----------|-------------|----------------------|
//! | JPEG | `.jpg`, `.jpeg` | *(always on)* | 10-bit (ravif auto) |
//! | PNG (8-bit) | `.png` | *(always on)* | 10-bit (ravif auto) |
//! | PNG (16-bit / HDR10) | `.png` | *(always on)* | **10-bit** (`encode_raw_planes_10_bit`) |
//! | WebP | `.webp` | *(always on)* | 10-bit (ravif auto) |
//! | HEIC / HEIF | `.heic`, `.heif` | `heic-experimental` | 10-bit (ravif auto) |
//!
//! ## HDR10
//!
//! **16-bit PNG** files (a standard still-image HDR10 format) are decoded at
//! full precision and encoded as genuine **10-bit AVIF** using ravif's
//! `encode_raw_planes_10_bit`.  Each 16-bit channel is scaled to 10 bits
//! (right-shift by 6) and then converted to YCbCr BT.601, preserving 1 024
//! distinct levels per channel instead of the 256 available in 8-bit output.
//!
//! > **Note on CICP metadata:** The AVIF colour primaries and transfer
//! > characteristics fields will reflect BT.601 / sRGB because ravif 0.13
//! > hardcodes those values in the raw-planes encoder path.  True HDR10 CICP
//! > metadata (BT.2020 primaries + PQ / HLG transfer) requires a future
//! > upgrade to a newer `rav1e` version.
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
//! | `dev-logging` | off | Structured pipeline logging via the [`log`](https://docs.rs/log) crate. Enable to get `DEBUG`/`INFO`/`WARN`/`ERROR` records from every pipeline stage. Zero cost when disabled. |
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
pub(crate) mod logging;

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
    /// | Format | Always available? | AVIF bit-depth |
    /// |--------|------------------|---------------|
    /// | JPEG / JPG | ✓ | 10-bit (ravif auto) |
    /// | PNG (8-bit) | ✓ | 10-bit (ravif auto) |
    /// | PNG (16-bit / HDR10) | ✓ | **10-bit** (raw planes) |
    /// | WebP | ✓ | 10-bit (ravif auto) |
    /// | HEIC / HEIF | `heic-experimental` feature only | 10-bit (ravif auto) |
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
    /// | [`Error::Encode`] | AVIF encoding failed or output failed structural validation |
    /// | [`Error::UnsupportedFormat`] | Format not supported in this build (e.g., HEIC without `heic-experimental`) |
    pub fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        use logging::{img_error, img_info};

        img_info!(
            "convert: starting — input {} bytes, quality={}, alpha_quality={}, speed={}, \
             strip_exif={}, max_input_bytes={}, max_pixels={}, memory_limit={}",
            input.len(),
            self.config.quality,
            self.config.alpha_quality,
            self.config.speed,
            self.config.strip_exif,
            self.config.max_input_bytes,
            self.config.max_pixels,
            self.config.memory_limit_bytes,
        );

        match self.run_pipeline(input) {
            Ok(avif) => {
                img_info!(
                    "convert: complete — {} bytes in, {} bytes out",
                    input.len(),
                    avif.len(),
                );
                Ok(avif)
            }
            Err(e) => {
                img_error!("convert: failed — {}", e);
                Err(e)
            }
        }
    }

    /// Inner pipeline: metadata strip → decode → memory guard → encode.
    ///
    /// Separated from `convert` so logging can wrap the entire pipeline
    /// without pushing the outer function over the line-count lint.
    fn run_pipeline(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        use logging::{img_debug, img_error, img_info, img_warn};

        if !self.config.strip_exif {
            img_warn!(
                "run_pipeline: strip_exif=false — metadata retention increases output size"
            );
            eprintln!(
                "img2avif: preserve_metadata is enabled — \
                 metadata retention increases output size and Lambda cost"
            );
        }

        // Reject oversized uploads before touching any bytes.
        if input.len() as u64 > self.config.max_input_bytes {
            img_error!(
                "run_pipeline: input {} bytes exceeds limit of {} bytes",
                input.len(),
                self.config.max_input_bytes,
            );
            return Err(Error::Decode(format!(
                "input too large: {} bytes exceeds the {}-byte limit",
                input.len(),
                self.config.max_input_bytes,
            )));
        }

        let guard = MemoryGuard::new(self.config.memory_limit_bytes);
        #[cfg(feature = "dev-logging")]
        if let Some(rss) = MemoryGuard::current_rss_bytes() {
            img_debug!("run_pipeline: pre-decode RSS = {} MiB", rss / (1024 * 1024));
        }
        #[allow(clippy::question_mark)] // explicit if-let preserves the log call
        if let Err(e) = guard.check() {
            img_error!("run_pipeline: pre-decode memory guard failed: {}", e);
            return Err(e);
        }

        // Strip metadata before decode so the image library never sees
        // potentially malformed APP / ancillary chunks.
        let processed: Vec<u8> = if self.config.strip_exif {
            let stripped = metadata::strip_metadata(input);
            img_debug!(
                "run_pipeline: metadata stripped — {} → {} bytes",
                input.len(),
                stripped.len()
            );
            stripped
        } else {
            input.to_vec()
        };

        img_debug!("run_pipeline: decoding {} bytes", processed.len());
        let raw = match decoder::decode(&processed, self.config.max_pixels) {
            Ok(r) => r,
            Err(e) => {
                img_error!("run_pipeline: decode failed: {}", e);
                return Err(e);
            }
        };

        img_info!(
            "run_pipeline: decoded — {}×{} px, {} format",
            raw.width,
            raw.height,
            match &raw.pixels {
                decoder::Pixels::Rgba8(_) => "8-bit RGBA",
                decoder::Pixels::Rgba16(_) => "16-bit RGBA (10-bit AVIF output)",
            }
        );

        // Post-decode RSS check: the pixel buffer is now live.
        #[cfg(feature = "dev-logging")]
        if let Some(rss) = MemoryGuard::current_rss_bytes() {
            img_debug!("run_pipeline: post-decode RSS = {} MiB", rss / (1024 * 1024));
        }
        #[allow(clippy::question_mark)] // explicit if-let preserves the log call
        if let Err(e) = guard.check() {
            img_error!("run_pipeline: post-decode memory guard failed: {}", e);
            return Err(e);
        }

        img_debug!(
            "run_pipeline: encoding {}×{} → AVIF q={} aq={} s={}",
            raw.width, raw.height,
            self.config.quality, self.config.alpha_quality, self.config.speed,
        );
        match encoder::encode_avif(
            &raw,
            self.config.quality,
            self.config.speed,
            self.config.alpha_quality,
        ) {
            Ok(avif) => Ok(avif),
            Err(e) => {
                img_error!("run_pipeline: encode failed: {}", e);
                Err(e)
            }
        }
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
        let cfg = Config::default().quality(200).alpha_quality(200).speed(99);
        assert_eq!(cfg.quality, 100);
        assert_eq!(cfg.alpha_quality, 100);
        assert_eq!(cfg.speed, 10);
        let cfg_low = Config::default().quality(0).alpha_quality(0);
        assert_eq!(cfg_low.quality, 1);
        assert_eq!(cfg_low.alpha_quality, 1);
    }

    #[test]
    fn config_accessor() {
        let cfg = Config::default().quality(42);
        let converter = Converter::new(cfg).unwrap();
        assert_eq!(converter.config().quality, 42);
    }
}
