use thiserror::Error;

/// All errors that can be returned by `img2avif`.
///
/// This enum is `#[non_exhaustive]`: match it with a wildcard arm (`_`) so
/// that your code continues to compile when new variants are added in future
/// minor versions.
///
/// # Example
///
/// ```rust,no_run
/// # use img2avif::{Config, Converter, Error};
/// # let input = vec![];
/// # let converter = Converter::new(Config::default()).unwrap();
/// match converter.convert(&input) {
///     Ok(avif) => println!("encoded {} bytes", avif.len()),
///     Err(Error::InputTooLarge { width, height, .. }) => {
///         eprintln!("image {width}×{height} is too large");
///     }
///     Err(e) => eprintln!("conversion failed: {e}"),
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    /// The input bytes could not be decoded as a recognised image format.
    #[error("failed to decode image: {0}")]
    Decode(String),

    /// AVIF encoding failed.
    #[error("failed to encode AVIF: {0}")]
    Encode(String),

    /// The image dimensions exceed the configured [`crate::Config::max_pixels`] limit.
    #[error(
        "input too large: {width}×{height} ({} pixels) exceeds the {max_pixels}-pixel limit",
        u64::from(*width) * u64::from(*height)
    )]
    InputTooLarge {
        /// Image width in pixels.
        width: u32,
        /// Image height in pixels.
        height: u32,
        /// The configured pixel limit.
        max_pixels: u64,
    },

    /// Peak RSS memory exceeded [`crate::Config::memory_limit_bytes`].
    ///
    /// Return this error immediately without further processing.
    #[error("memory limit exceeded: {used_mb} MB used, limit is {limit_mb} MB")]
    MemoryExceeded {
        /// Observed RSS in megabytes at the time the limit was breached.
        used_mb: u64,
        /// Configured limit in megabytes.
        limit_mb: u64,
    },

    /// An I/O error occurred.
    ///
    /// Implementing `From<std::io::Error>` allows callers to use `?` on I/O
    /// operations when building on top of this library.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The requested format requires an optional feature flag that was not
    /// enabled at compile time.
    ///
    /// Enable the `heic-experimental` or `raw-experimental` feature to add
    /// support for those formats.
    #[error(
        "format not supported in this build: {0} \
         (enable the corresponding Cargo feature flag)"
    )]
    UnsupportedFormat(String),

    /// An unexpected internal error occurred.
    ///
    /// Please [file a bug report] if you see this variant in production.
    ///
    /// [file a bug report]: https://github.com/Canter-Equine/img2avif/issues
    #[error("internal error: {0}")]
    Internal(String),
}
