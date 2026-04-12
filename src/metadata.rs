use img_parts::{webp::WebP, Bytes, ImageEXIF, ImageICC};

/// Image container format detected from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Unknown,
}

/// Detect the image format from the first few magic bytes without allocating.
///
/// - JPEG: `FF D8`
/// - PNG: `89 50 4E 47 0D 0A 1A 0A`
/// - WebP: `52 49 46 46 … 57 45 42 50` (`RIFF….WEBP`)
fn detect_format(data: &[u8]) -> ImageFormat {
    if data.starts_with(&[0xFF, 0xD8]) {
        return ImageFormat::Jpeg;
    }
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return ImageFormat::Png;
    }
    // WebP: 4-byte RIFF tag + 4-byte size + 4-byte "WEBP" form type.
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return ImageFormat::WebP;
    }
    ImageFormat::Unknown
}

/// Strip EXIF, IPTC, and XMP metadata from `data`.
///
/// Supported container formats:
///
/// - **JPEG** — removes APP1 (EXIF/XMP), APP2 (ICC), APP13 (IPTC), APP14 (Adobe)
/// - **PNG** — removes `tEXt`, `zTXt`, `iTXt`, and `eXIf` chunks
/// - **WebP** — removes EXIF, ICC, and XMP chunks via the RIFF container
///
/// Returns `Some(stripped_bytes)` on success, or `None` if the format is not
/// recognised (e.g. HEIC/HEIF, RAW files).  The caller is responsible for
/// deciding whether an unrecognised format is acceptable — for example,
/// returning [`crate::Error::UnsupportedFormat`] when `strip_exif = true`
/// was requested.
///
/// For **recognised** formats (JPEG, PNG, WebP) the function always returns
/// `Some`: if the container parser fails for a malformed-but-detected file the
/// original bytes are returned unchanged, rather than propagating `None` and
/// incorrectly telling the caller that the format is unsupported.
///
/// The format is detected from magic bytes before any allocation occurs, so
/// `Bytes::copy_from_slice` is called **at most once** per invocation.
pub fn strip_metadata(data: &[u8]) -> Option<Vec<u8>> {
    match detect_format(data) {
        ImageFormat::Jpeg => {
            let Ok(mut jpeg) = img_parts::jpeg::Jpeg::from_bytes(Bytes::copy_from_slice(data))
            else {
                // Format was detected as JPEG but img_parts failed to parse it.
                // Pass the data through unchanged so the decoder (not the strip
                // step) surfaces the correct error.
                return Some(data.to_vec());
            };
            jpeg.set_exif(None);
            jpeg.set_icc_profile(None);
            jpeg.segments_mut()
                .retain(|seg| !is_stripped_jpeg_marker(seg.marker()));
            Some(jpeg.encoder().bytes().to_vec())
        }
        ImageFormat::Png => {
            let Ok(mut png) = img_parts::png::Png::from_bytes(Bytes::copy_from_slice(data)) else {
                return Some(data.to_vec());
            };
            png.chunks_mut().retain(|chunk| {
                let tag = chunk.kind();
                tag != *b"tEXt" && tag != *b"zTXt" && tag != *b"iTXt" && tag != *b"eXIf"
            });
            Some(png.encoder().bytes().to_vec())
        }
        ImageFormat::WebP => {
            let Ok(mut webp) = WebP::from_bytes(Bytes::copy_from_slice(data)) else {
                return Some(data.to_vec());
            };
            webp.set_exif(None);
            webp.set_icc_profile(None);
            // Remove the XMP chunk (four-CC `XMP `) if present.
            webp.chunks_mut()
                .retain(|chunk| chunk.id() != img_parts::webp::CHUNK_XMP);
            Some(webp.encoder().bytes().to_vec())
        }
        // Fallthrough: format not recognised (e.g. HEIC/HEIF, RAW files).
        // Return None so callers with `strip_exif = true` can surface an
        // explicit error rather than silently leaking metadata.
        ImageFormat::Unknown => None,
    }
}

/// Returns `true` for JPEG APP markers that carry only metadata.
///
/// - `0xE1` APP1 — EXIF or XMP  
/// - `0xE2` APP2 — ICC colour profile (the parsed form is cleared via
///   `set_icc_profile(None)`; this catches any residual raw segments)
/// - `0xED` APP13 — IPTC / Photoshop
/// - `0xEE` APP14 — Adobe colour metadata
fn is_stripped_jpeg_marker(marker: u8) -> bool {
    matches!(marker, 0xE1 | 0xE2 | 0xED | 0xEE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_format_jpeg() {
        assert_eq!(detect_format(&[0xFF, 0xD8, 0x00]), ImageFormat::Jpeg);
    }

    #[test]
    fn detect_format_png() {
        let magic = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        assert_eq!(detect_format(&magic), ImageFormat::Png);
    }

    #[test]
    fn detect_format_webp() {
        let mut hdr = [0u8; 12];
        hdr[..4].copy_from_slice(b"RIFF");
        hdr[8..12].copy_from_slice(b"WEBP");
        assert_eq!(detect_format(&hdr), ImageFormat::WebP);
    }

    #[test]
    fn detect_format_unknown() {
        assert_eq!(detect_format(b"not an image"), ImageFormat::Unknown);
        assert_eq!(detect_format(&[]), ImageFormat::Unknown);
    }

    #[test]
    fn strip_unknown_bytes_returns_none() {
        let data = b"not an image";
        assert!(strip_metadata(data).is_none());
    }

    #[test]
    fn strip_empty_slice_returns_none() {
        assert!(strip_metadata(&[]).is_none());
    }

    #[test]
    fn marker_classification() {
        assert!(is_stripped_jpeg_marker(0xE1)); // EXIF/XMP
        assert!(is_stripped_jpeg_marker(0xE2)); // ICC
        assert!(is_stripped_jpeg_marker(0xED)); // IPTC
        assert!(is_stripped_jpeg_marker(0xEE)); // Adobe
        assert!(!is_stripped_jpeg_marker(0xE0)); // APP0/JFIF — kept
        assert!(!is_stripped_jpeg_marker(0xDA)); // SOS — kept
    }

    /// Round-trip: encode a WebP, call `strip_metadata`, verify the result
    /// still decodes as a valid image (i.e. the image data is intact).
    ///
    /// This is a structural sanity check; full EXIF-injection + verification
    /// would require a test fixture with injected metadata.
    #[test]
    fn strip_webp_does_not_corrupt_image() {
        // Build a minimal WebP using the image crate.
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([100u8, 150, 200, 255]));
        let mut buf = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::WebP,
        )
        .expect("test WebP encode");

        let stripped = strip_metadata(&buf).expect("WebP stripping should succeed");
        // The stripped bytes must still be parseable as a WebP.
        assert!(
            WebP::from_bytes(Bytes::copy_from_slice(&stripped)).is_ok(),
            "stripped WebP is not parseable"
        );
        // Verify it still decodes as an image.
        let decoded = image::load_from_memory(&stripped).expect("stripped WebP should decode");
        assert_eq!(decoded.width(), 8);
        assert_eq!(decoded.height(), 8);
    }

    /// Malformed JPEG/PNG/WebP data (correct magic bytes but corrupt body)
    /// must return `Some` (pass-through), not `None`.
    ///
    /// Returning `None` would cause the caller to emit an `UnsupportedFormat`
    /// error, which is incorrect for a format we actually recognise.
    #[test]
    fn malformed_jpeg_returns_some_passthrough() {
        // JPEG magic bytes followed by garbage.
        let data = [0xFF, 0xD8, 0xDE, 0xAD, 0xBE, 0xEF, 0x00];
        let result = strip_metadata(&data);
        assert!(
            result.is_some(),
            "malformed-but-detected JPEG should return Some, not None"
        );
        assert_eq!(
            result.unwrap(),
            data,
            "passthrough should be identical to input"
        );
    }

    #[test]
    fn malformed_png_returns_some_passthrough() {
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        data.extend_from_slice(b"GARBAGE");
        let result = strip_metadata(&data);
        assert!(
            result.is_some(),
            "malformed-but-detected PNG should return Some, not None"
        );
        assert_eq!(
            result.unwrap(),
            data,
            "passthrough should be identical to input"
        );
    }

    #[test]
    fn malformed_webp_returns_some_passthrough() {
        let mut data = [0u8; 20];
        data[..4].copy_from_slice(b"RIFF");
        data[8..12].copy_from_slice(b"WEBP");
        // Rest is zeros — img_parts will fail to parse this as a valid WebP.
        let result = strip_metadata(&data);
        assert!(
            result.is_some(),
            "malformed-but-detected WebP should return Some, not None"
        );
        assert_eq!(
            result.unwrap(),
            data,
            "passthrough should be identical to input"
        );
    }
}
