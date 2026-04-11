//! JPEG decoder stub.
//!
//! Decoding is delegated to the `image` crate via [`super::decode`].
//! This module exists as a named extension point for future JPEG-specific
//! pre-processing (e.g. chroma-subsampling hints, progressive loading).

/// JPEG-specific decoder handle.
///
/// Currently a zero-sized type; all decoding goes through [`super::decode`].
#[allow(dead_code)]
pub struct JpegDecoder;
