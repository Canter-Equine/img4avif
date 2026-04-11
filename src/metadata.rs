use img_parts::{webp::WebP, Bytes, ImageEXIF, ImageICC};

/// Strip EXIF, IPTC, and XMP metadata from `data`.
///
/// Supported container formats:
///
/// - **JPEG** — removes APP1 (EXIF/XMP), APP2 (ICC), APP13 (IPTC), APP14 (Adobe)
/// - **PNG** — removes `tEXt`, `zTXt`, `iTXt`, and `eXIf` chunks
/// - **WebP** — removes EXIF, ICC, and XMP chunks via the RIFF container
///
/// For any other format the bytes are returned unchanged (best-effort).
///
/// The format is detected from magic bytes so that only a single parse attempt
/// is made, avoiding redundant copies and failed parses for the two formats
/// that don't match.
pub fn strip_metadata(data: &[u8]) -> Vec<u8> {
    // JPEG: SOI marker 0xFF 0xD8
    if data.starts_with(b"\xFF\xD8") {
        if let Ok(mut jpeg) = img_parts::jpeg::Jpeg::from_bytes(Bytes::copy_from_slice(data)) {
            jpeg.set_exif(None);
            jpeg.set_icc_profile(None);
            jpeg.segments_mut()
                .retain(|seg| !is_stripped_jpeg_marker(seg.marker()));
            return jpeg.encoder().bytes().to_vec();
        }
    }

    // PNG: 8-byte signature \x89PNG\r\n\x1a\n
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        if let Ok(mut png) = img_parts::png::Png::from_bytes(Bytes::copy_from_slice(data)) {
            png.chunks_mut().retain(|chunk| {
                let tag = chunk.kind();
                tag != *b"tEXt" && tag != *b"zTXt" && tag != *b"iTXt" && tag != *b"eXIf"
            });
            return png.encoder().bytes().to_vec();
        }
    }

    // WebP: RIFF....WEBP
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        if let Ok(mut webp) = WebP::from_bytes(Bytes::copy_from_slice(data)) {
            webp.set_exif(None);
            webp.set_icc_profile(None);
            // Remove the XMP chunk (four-CC `XMP `) if present.
            webp.chunks_mut()
                .retain(|chunk| chunk.id() != img_parts::webp::CHUNK_XMP);
            return webp.encoder().bytes().to_vec();
        }
    }

    data.to_vec()
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
    fn strip_unknown_bytes_returns_copy() {
        let data = b"not an image";
        assert_eq!(strip_metadata(data), data.to_vec());
    }

    #[test]
    fn strip_empty_slice() {
        assert_eq!(strip_metadata(&[]), Vec::<u8>::new());
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

        let stripped = strip_metadata(&buf);
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
}
