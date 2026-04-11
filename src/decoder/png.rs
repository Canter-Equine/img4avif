//! PNG decoder stub.
//!
//! Decoding is delegated to the `image` crate via [`super::decode`].
//! This module exists as a named extension point for future PNG-specific
//! pre-processing (e.g. gamma correction, background colour handling).

/// PNG-specific decoder handle.
///
/// Currently a zero-sized type; all decoding goes through [`super::decode`].
#[allow(dead_code)]
pub struct PngDecoder;
