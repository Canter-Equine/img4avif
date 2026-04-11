use img_parts::{Bytes, ImageEXIF, ImageICC};

/// Strip EXIF, IPTC, and XMP metadata from `data`.
///
/// Supported container formats:
///
/// - **JPEG** — removes APP1 (EXIF/XMP), APP2 (ICC), APP13 (IPTC), APP14 (Adobe)
/// - **PNG** — removes `tEXt`, `zTXt`, `iTXt`, and `eXIf` chunks
///
/// For any other format the bytes are returned unchanged (best-effort).
pub fn strip_metadata(data: &[u8]) -> Vec<u8> {
    if let Ok(mut jpeg) = img_parts::jpeg::Jpeg::from_bytes(Bytes::copy_from_slice(data)) {
        jpeg.set_exif(None);
        jpeg.set_icc_profile(None);
        jpeg.segments_mut()
            .retain(|seg| !is_stripped_jpeg_marker(seg.marker()));
        return jpeg.encoder().bytes().to_vec();
    }

    if let Ok(mut png) = img_parts::png::Png::from_bytes(Bytes::copy_from_slice(data)) {
        png.chunks_mut().retain(|chunk| {
            let tag = chunk.kind();
            tag != *b"tEXt" && tag != *b"zTXt" && tag != *b"iTXt" && tag != *b"eXIf"
        });
        return png.encoder().bytes().to_vec();
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
}
