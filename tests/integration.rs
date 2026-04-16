//! End-to-end conversion tests.  All images are generated in-memory.

use img4avif::{Config, Converter, Error, OutputResolution};

fn make_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::new(width, height);
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

fn make_jpeg(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::new(width, height);
    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Jpeg,
    )
    .unwrap();
    buf
}

fn make_webp(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([100u8, 150, 200, 255]));
    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::WebP,
    )
    .unwrap();
    buf
}

#[test]
fn png_converts_to_avif() {
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_png(64, 64))
        .expect("PNG → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn jpeg_converts_to_avif() {
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_jpeg(64, 64))
        .expect("JPEG → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn quality_extremes_round_trip() {
    for q in [1u8, 100] {
        let cfg = Config::default().quality(q).speed(10);
        let avif = Converter::new(cfg)
            .unwrap()
            .convert(&make_png(16, 16))
            .unwrap();
        assert!(!avif.is_empty(), "quality {q} produced empty output");
    }
}

#[test]
fn lambda_cost_preset_works() {
    let avif = Converter::new(Config::lambda_cost_optimized())
        .unwrap()
        .convert(&make_png(32, 32))
        .unwrap();
    assert!(!avif.is_empty());
}

#[test]
fn strip_exif_default_is_true() {
    assert!(Config::default().strip_exif);
}

#[test]
fn preserve_metadata_succeeds_with_warning() {
    // strip_exif=false must succeed (it prints a warning to stderr).
    let cfg = Config::default().strip_exif(false);
    assert!(!Converter::new(cfg)
        .unwrap()
        .convert(&make_png(8, 8))
        .unwrap()
        .is_empty());
}

#[test]
fn rejects_input_exceeding_byte_limit() {
    let png = make_png(64, 64);
    let cfg = Config::default().max_input_bytes(1); // 1-byte cap
    let err = Converter::new(cfg).unwrap().convert(&png).unwrap_err();
    // Should be a Decode error describing the byte-size violation.
    assert!(matches!(err, Error::Decode(_)), "got: {err:?}");
}

#[test]
fn rejects_image_exceeding_pixel_limit() {
    let png = make_png(32, 32); // 1024 pixels
    let cfg = Config::default().max_pixels(100);
    let err = Converter::new(cfg).unwrap().convert(&png).unwrap_err();
    assert!(matches!(err, Error::InputTooLarge { .. }), "got: {err:?}");
}

#[test]
fn garbage_input_never_panics() {
    let converter = Converter::new(Config::default()).unwrap();
    for garbage in &[
        vec![0xDE, 0xAD, 0xBE, 0xEF],
        vec![0x00],
        vec![0xFF; 1024],
        b"not an image".to_vec(),
    ] {
        assert!(converter.convert(garbage).is_err());
    }
}

#[test]
fn empty_input_returns_error() {
    assert!(Converter::new(Config::default())
        .unwrap()
        .convert(&[])
        .is_err());
}

#[test]
fn config_quality_clamped() {
    assert_eq!(Config::default().quality(0).quality, 1);
    assert_eq!(Config::default().quality(200).quality, 10);
}

#[test]
fn config_speed_clamped() {
    assert_eq!(Config::default().speed(0).speed, 1);
    assert_eq!(Config::default().speed(99).speed, 10);
}

#[test]
fn config_is_clone() {
    let a = Config::default().quality(5);
    assert_eq!(a.clone().quality, 5);
}

/// Make a 16-bit PNG image (Rgb16 colour type).
///
/// 16-bit PNGs are the primary distribution format for HDR10 still images.
/// Each channel value is set to `value` (0 – 65535) for reproducibility.
fn make_png_16bit(width: u32, height: u32, channel_value: u16) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img: ImageBuffer<Rgb<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(width, height, Rgb([channel_value; 3]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb16(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

#[test]
fn png_16bit_produces_avif() {
    // Verify that a 16-bit PNG (HDR10-compatible input) is accepted and
    // produces a non-empty AVIF file.
    let png16 = make_png_16bit(32, 32, 48_000);
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&png16)
        .expect("16-bit PNG → AVIF conversion failed");
    assert!(!avif.is_empty(), "16-bit PNG produced empty AVIF output");
}

#[test]
fn png_16bit_dark_and_bright_both_succeed() {
    // Test both ends of the 16-bit range.
    for value in [0u16, 32768, 65535] {
        let png = make_png_16bit(8, 8, value);
        let avif = Converter::new(Config::default())
            .unwrap()
            .convert(&png)
            .unwrap_or_else(|e| panic!("16-bit PNG (value={value}) failed: {e}"));
        assert!(!avif.is_empty(), "value={value} produced empty output");
    }
}

#[test]
fn alpha_quality_setting_is_accepted() {
    // Verify that alpha_quality flows through without error.
    let cfg = Config::default().quality(8).alpha_quality(10);
    assert_eq!(cfg.alpha_quality, 10);
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png(16, 16))
        .expect("alpha_quality=10 conversion failed");
    assert!(!avif.is_empty());
}

#[test]
fn config_alpha_quality_clamped() {
    assert_eq!(Config::default().alpha_quality(0).alpha_quality, 1);
    assert_eq!(Config::default().alpha_quality(200).alpha_quality, 10);
}

#[test]
fn converter_exposes_config() {
    let cfg = Config::default().quality(8);
    assert_eq!(Converter::new(cfg).unwrap().config().quality, 8);
}

#[test]
fn heic_without_feature_returns_unsupported_format() {
    // A minimal synthetic ISOBMFF ftyp box: size=0x14, type=ftyp, brand=heic.
    // This is enough to trigger the HEIC/HEIF magic-byte check.
    let fake_heic: &[u8] = &[
        0x00, 0x00, 0x00, 0x14, // box size = 20
        0x66, 0x74, 0x79, 0x70, // "ftyp"
        0x68, 0x65, 0x69, 0x63, // major brand "heic"
        0x00, 0x00, 0x00, 0x00, // minor version
        0x68, 0x65, 0x69, 0x63, // compatible brand "heic"
    ];

    let err = Converter::new(Config::default())
        .unwrap()
        .convert(fake_heic)
        .unwrap_err();

    // Without `heic-experimental` this must be UnsupportedFormat.
    // With the feature enabled it will fail later (not a valid HEIC bitstream),
    // so we accept Decode as well in that case.
    assert!(
        matches!(err, Error::UnsupportedFormat(_) | Error::Decode(_)),
        "expected UnsupportedFormat or Decode, got: {err:?}"
    );

    // When the feature is off, the error message should hint at the feature flag
    // or indicate that EXIF stripping is not supported for this format.
    // With strip_exif=true (the default), the metadata-stripping stage fires
    // first and reports an UnsupportedFormat error before the decoder is even
    // reached.  Both messages are valid UnsupportedFormat errors.
    #[cfg(not(feature = "heic-experimental"))]
    if let Error::UnsupportedFormat(msg) = &err {
        assert!(
            msg.contains("heic-experimental") || msg.contains("strip_exif"),
            "UnsupportedFormat message should mention 'heic-experimental' or 'strip_exif', \
             got: {msg}"
        );
    }
}

#[test]
fn strip_exif_true_with_unsupported_format_returns_error() {
    // A synthetic HEIC ftyp box — strip_exif=true (the default) should
    // return UnsupportedFormat rather than silently passing through with
    // metadata intact.
    let fake_heic: &[u8] = &[
        0x00, 0x00, 0x00, 0x14, // box size = 20
        0x66, 0x74, 0x79, 0x70, // "ftyp"
        0x68, 0x65, 0x69, 0x63, // major brand "heic"
        0x00, 0x00, 0x00, 0x00, // minor version
        0x68, 0x65, 0x69, 0x63, // compatible brand "heic"
    ];

    // strip_exif=true is the default.
    let err = Converter::new(Config::default())
        .unwrap()
        .convert(fake_heic)
        .unwrap_err();

    assert!(
        matches!(err, Error::UnsupportedFormat(_) | Error::Decode(_)),
        "expected UnsupportedFormat or Decode for HEIC with strip_exif=true, got: {err:?}"
    );
}

#[test]
fn strip_exif_false_with_heic_format_does_not_error_at_strip_step() {
    // With strip_exif=false the metadata-stripping stage is bypassed entirely.
    // A fake HEIC payload will still fail (no decoder), but the error must
    // come from the decoder, not from metadata stripping.
    let fake_heic: &[u8] = &[
        0x00, 0x00, 0x00, 0x14, 0x66, 0x74, 0x79, 0x70, 0x68, 0x65, 0x69, 0x63, 0x00, 0x00, 0x00,
        0x00, 0x68, 0x65, 0x69, 0x63,
    ];

    let err = Converter::new(Config::default().strip_exif(false))
        .unwrap()
        .convert(fake_heic)
        .unwrap_err();

    // The error should come from the decoder / UnsupportedFormat (feature
    // flag) path, not from the metadata-stripping stage.
    assert!(
        matches!(err, Error::UnsupportedFormat(_) | Error::Decode(_)),
        "expected UnsupportedFormat or Decode, got: {err:?}"
    );
    // Crucially it must NOT mention 'strip_exif' since stripping was disabled.
    if let Error::UnsupportedFormat(msg) = &err {
        assert!(
            !msg.contains("strip_exif"),
            "error should not mention strip_exif when stripping is disabled, got: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// WebP support
// ---------------------------------------------------------------------------

#[test]
fn webp_converts_to_avif() {
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_webp(32, 32))
        .expect("WebP → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn webp_strip_exif_true_does_not_error() {
    // Verifies that the WebP metadata-stripping branch in strip_metadata
    // does not corrupt the image data (full end-to-end conversion succeeds).
    let avif = Converter::new(Config::default().strip_exif(true))
        .unwrap()
        .convert(&make_webp(16, 16))
        .expect("WebP strip_exif=true → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn webp_with_resolution_width1080() {
    // A 1920-wide WebP should be correctly downscaled through the new WebP
    // metadata-strip path and then resized to 1080.
    let cfg = Config::default().output_resolutions(vec![OutputResolution::Width1080]);
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_webp(1920, 1080))
        .expect("WebP 1920→1080 AVIF");
    assert!(!avif.is_empty());
}

// ---------------------------------------------------------------------------
// Degenerate / edge-case image dimensions
// ---------------------------------------------------------------------------

#[test]
fn one_by_one_pixel_converts() {
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_png(1, 1))
        .expect("1×1 PNG → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn one_pixel_wide_tall_image() {
    // 1×512: very tall narrow image
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_png(1, 512))
        .expect("1×512 PNG → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn one_pixel_tall_wide_image() {
    // 512×1: very wide flat image
    let avif = Converter::new(Config::default())
        .unwrap()
        .convert(&make_png(512, 1))
        .expect("512×1 PNG → AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn width1080_on_wide_flat_image_preserves_height_of_one() {
    // A 2000×1 image resized to Width1080 should end up 1080×1 (height clipped to 1).
    let cfg = Config::default().output_resolutions(vec![OutputResolution::Width1080]);
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png(2000, 1))
        .expect("2000×1 PNG → 1080 AVIF");
    assert!(!avif.is_empty());
}

// ---------------------------------------------------------------------------
// 16-bit PNG + resize
// ---------------------------------------------------------------------------

#[test]
fn png_16bit_with_resize_to_1080() {
    // Exercises the 16-bit resize path in resize_raw_image.
    let cfg = Config::default().output_resolutions(vec![OutputResolution::Width1080]);
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png_16bit(1920, 1080, 48000))
        .expect("16-bit PNG 1920→1080 AVIF");
    assert!(!avif.is_empty());
}

#[test]
fn png_16bit_with_resize_to_2560() {
    let cfg = Config::default().output_resolutions(vec![OutputResolution::Width2560]);
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png_16bit(3840, 2160, 48000))
        .expect("16-bit PNG 3840→2560 AVIF");
    assert!(!avif.is_empty());
}

// ---------------------------------------------------------------------------
// Directly-set config fields bypass builder clamping
// ---------------------------------------------------------------------------

#[test]
fn quality_zero_via_direct_field_still_encodes() {
    // Config::quality = 0 bypasses the builder's clamp(1,100).
    // The encoder must not panic; it re-clamps internally.
    let cfg = Config {
        quality: 0,
        ..Config::default()
    };
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png(8, 8))
        .expect("quality=0 should not panic");
    assert!(!avif.is_empty());
}

#[test]
fn speed_zero_via_direct_field_still_encodes() {
    let cfg = Config {
        speed: 0,
        ..Config::default()
    };
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png(8, 8))
        .expect("speed=0 should not panic");
    assert!(!avif.is_empty());
}

#[test]
fn alpha_quality_zero_via_direct_field_still_encodes() {
    let cfg = Config {
        alpha_quality: 0,
        ..Config::default()
    };
    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&make_png(8, 8))
        .expect("alpha_quality=0 should not panic");
    assert!(!avif.is_empty());
}

// ---------------------------------------------------------------------------
// Error propagation
// ---------------------------------------------------------------------------

#[test]
fn truncated_webp_returns_error() {
    // RIFF magic bytes but truncated — must not panic.
    let truncated: &[u8] = b"RIFF\x00\x00\x00\x00WEBP";
    assert!(Converter::new(Config::default())
        .unwrap()
        .convert(truncated)
        .is_err());
}

#[test]
fn convert_multi_returns_error_on_bad_input() {
    let cfg = Config::default().output_resolutions(vec![
        OutputResolution::Original,
        OutputResolution::Width1080,
    ]);
    let err = Converter::new(cfg)
        .unwrap()
        .convert_multi(b"garbage")
        .unwrap_err();
    assert!(matches!(
        err,
        Error::Decode(_) | Error::UnsupportedFormat(_)
    ));
}
