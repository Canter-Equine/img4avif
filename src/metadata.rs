use img_parts::{Bytes, ImageEXIF, ImageICC};

/// Strip all EXIF, IPTC, and XMP metadata from raw image bytes.
///
/// Supports **JPEG** (removes APP1/APP2/APP13/APP14 segments) and **PNG**
/// (removes `tEXt`, `zTXt`, `iTXt`, and `eXIf` chunks).  For other formats
/// the bytes are returned unchanged — the metadata contract is best-effort.
///
/// This is called automatically by [`Converter::convert`] when
/// [`Config::strip_exif`] is `true` (the default).
///
/// [`Converter::convert`]: crate::Converter::convert
/// [`Config::strip_exif`]: crate::Config::strip_exif
pub fn strip_metadata(data: &[u8]) -> Vec<u8> {
    // ── JPEG ──────────────────────────────────────────────────────────────
    if let Ok(mut jpeg) = img_parts::jpeg::Jpeg::from_bytes(Bytes::copy_from_slice(data)) {
        jpeg.set_exif(None);
        jpeg.set_icc_profile(None);
        // Remove APP1 (XMP), APP13 (IPTC/Photoshop), APP14 (Adobe).
        jpeg.segments_mut()
            .retain(|seg| !is_metadata_jpeg_marker(seg.marker()));
        return jpeg.encoder().bytes().to_vec();
    }

    // ── PNG ───────────────────────────────────────────────────────────────
    if let Ok(mut png) = img_parts::png::Png::from_bytes(Bytes::copy_from_slice(data)) {
        png.chunks_mut().retain(|chunk| {
            let tag = chunk.kind();
            // Remove tEXt, zTXt, iTXt (text metadata) and eXIf (EXIF).
            tag != *b"tEXt" && tag != *b"zTXt" && tag != *b"iTXt" && tag != *b"eXIf"
        });
        return png.encoder().bytes().to_vec();
    }

    // ── Unknown format — return as-is ─────────────────────────────────────
    data.to_vec()
}

/// Returns `true` for JPEG APP markers that carry metadata.
///
/// Retained markers:
/// - APP0 (`0xE0`) — JFIF header, kept for decoder compatibility
/// - APP2 that is NOT ICC — handled separately by `set_icc_profile`
///
/// Removed markers:
/// - APP1 (`0xE1`) — EXIF or XMP namespace
/// - APP2 (`0xE2`) — ICC colour profile (stripped via `set_icc_profile`)
/// - APP13 (`0xED`) — IPTC / Photoshop metadata
/// - APP14 (`0xEE`) — Adobe metadata
fn is_metadata_jpeg_marker(marker: u8) -> bool {
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
        // Metadata markers should be stripped.
        assert!(is_metadata_jpeg_marker(0xE1)); // EXIF/XMP
        assert!(is_metadata_jpeg_marker(0xE2)); // ICC
        assert!(is_metadata_jpeg_marker(0xED)); // IPTC
        assert!(is_metadata_jpeg_marker(0xEE)); // Adobe
                                                // Image-data markers must be kept.
        assert!(!is_metadata_jpeg_marker(0xE0)); // APP0 / JFIF
        assert!(!is_metadata_jpeg_marker(0xDA)); // SOS
        assert!(!is_metadata_jpeg_marker(0xDB)); // DQT
    }
}
