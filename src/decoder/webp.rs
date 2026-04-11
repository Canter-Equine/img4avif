//! WebP decoder stub.
//!
//! Decoding is delegated to the `image` crate via [`super::decode`].
//! This module exists as a named extension point for future WebP-specific
//! pre-processing (e.g. animation frame extraction).

/// WebP-specific decoder handle.
///
/// Currently a zero-sized type; all decoding goes through [`super::decode`].
#[allow(dead_code)]
pub struct WebpDecoder;
