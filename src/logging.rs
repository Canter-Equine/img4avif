//! Zero-cost conditional logging macros for `img2avif`.
//!
//! When the `dev-logging` Cargo feature is enabled these macros delegate to the
//! [`log`] crate facade, which means any compatible subscriber (e.g.
//! [`env_logger`](https://docs.rs/env_logger), [`tracing-log`](https://docs.rs/tracing-log),
//! or [`simplelog`](https://docs.rs/simplelog)) will receive structured records.
//!
//! When `dev-logging` is **disabled** (the default) every macro expands to a
//! unit expression `()` — the compiler removes them entirely, so there is
//! literally zero runtime cost.
//!
//! # Filtering
//!
//! All records are emitted under the `img2avif` target.  To see only this
//! library's logs with `env_logger`:
//!
//! ```sh
//! RUST_LOG=img2avif=debug cargo run
//! ```
//!
//! # Levels used by this library
//!
//! | Level | Used for |
//! |-------|---------|
//! | `ERROR` | Error paths — emitted just before returning `Err(…)` |
//! | `WARN` | Non-fatal surprises (metadata preserved, suspicious output size) |
//! | `INFO` | Per-image pipeline milestones (decoded, encoded, compression ratio) |
//! | `DEBUG` | Detailed sub-step data (pixel count, quality settings, RSS readings) |

/// Emit a `DEBUG`-level log record when the `dev-logging` feature is enabled.
///
/// Has zero cost when the feature is off — the call is erased by the compiler.
macro_rules! img_debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-logging")]
        ::log::debug!(target: "img2avif", $($arg)*);
    };
}

/// Emit an `INFO`-level log record when the `dev-logging` feature is enabled.
macro_rules! img_info {
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-logging")]
        ::log::info!(target: "img2avif", $($arg)*);
    };
}

/// Emit a `WARN`-level log record when the `dev-logging` feature is enabled.
macro_rules! img_warn {
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-logging")]
        ::log::warn!(target: "img2avif", $($arg)*);
    };
}

/// Emit an `ERROR`-level log record when the `dev-logging` feature is enabled.
macro_rules! img_error {
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-logging")]
        ::log::error!(target: "img2avif", $($arg)*);
    };
}

// Make macros available throughout the crate.
pub(crate) use img_debug;
pub(crate) use img_error;
pub(crate) use img_info;
pub(crate) use img_warn;
