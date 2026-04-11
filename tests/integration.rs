//! End-to-end conversion tests.
//!
//! All tests generate synthetic images in-memory so no fixtures are needed.

use img2avif::{Config, Converter, Error};

// ── Helpers ───────────────────────────────────────────────────────────────

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

// ── Happy-path tests ──────────────────────────────────────────────────────

#[test]
fn png_converts_to_avif() {
    let png = make_png(64, 64);
    let converter = Converter::new(Config::default()).unwrap();
    let avif = converter.convert(&png).expect("PNG → AVIF failed");
    assert!(!avif.is_empty(), "AVIF output must not be empty");
}

#[test]
fn jpeg_converts_to_avif() {
    let jpeg = make_jpeg(64, 64);
    let converter = Converter::new(Config::default()).unwrap();
    let avif = converter.convert(&jpeg).expect("JPEG → AVIF failed");
    assert!(!avif.is_empty());
}

#[test]
fn quality_100_roundtrip() {
    let png = make_png(16, 16);
    let config = Config::default().quality(100).speed(10);
    let converter = Converter::new(config).unwrap();
    assert!(!converter.convert(&png).unwrap().is_empty());
}

#[test]
fn quality_1_roundtrip() {
    let png = make_png(16, 16);
    let config = Config::default().quality(1).speed(10);
    let converter = Converter::new(config).unwrap();
    assert!(!converter.convert(&png).unwrap().is_empty());
}

#[test]
fn lambda_cost_preset_works() {
    let png = make_png(32, 32);
    let converter = Converter::new(Config::lambda_cost_optimized()).unwrap();
    assert!(!converter.convert(&png).unwrap().is_empty());
}

// ── EXIF / metadata tests ─────────────────────────────────────────────────

#[test]
fn strip_exif_default_is_true() {
    assert!(
        Config::default().strip_exif,
        "strip_exif must default to true"
    );
}

#[test]
fn preserve_metadata_flag_succeeds() {
    let png = make_png(8, 8);
    // strip_exif=false triggers a warning on stderr but must not fail.
    let config = Config::default().strip_exif(false);
    let converter = Converter::new(config).unwrap();
    assert!(!converter.convert(&png).unwrap().is_empty());
}

// ── Error-path tests ──────────────────────────────────────────────────────

#[test]
fn dimension_limit_enforced() {
    let png = make_png(32, 32); // 1024 pixels
    let config = Config::default().max_pixels(100); // limit below 1024
    let converter = Converter::new(config).unwrap();
    let err = converter.convert(&png).unwrap_err();
    assert!(
        matches!(err, Error::InputTooLarge { .. }),
        "expected InputTooLarge, got: {err:?}"
    );
}

#[test]
fn garbage_input_returns_error_not_panic() {
    // Covers the "never panic on malformed input" requirement.
    for garbage in &[
        vec![0xDE, 0xAD, 0xBE, 0xEF],
        vec![0x00],
        vec![0xFF; 1024],
        b"this is not an image".to_vec(),
    ] {
        let converter = Converter::new(Config::default()).unwrap();
        let result = converter.convert(garbage);
        assert!(
            result.is_err(),
            "expected an error for garbage input, got Ok"
        );
    }
}

#[test]
fn empty_input_returns_error_not_panic() {
    let converter = Converter::new(Config::default()).unwrap();
    assert!(converter.convert(&[]).is_err());
}

// ── Config builder tests ──────────────────────────────────────────────────

#[test]
fn config_quality_clamped_to_range() {
    assert_eq!(Config::default().quality(0).quality, 1);
    assert_eq!(Config::default().quality(200).quality, 100);
    assert_eq!(Config::default().quality(50).quality, 50);
}

#[test]
fn config_speed_clamped_to_range() {
    assert_eq!(Config::default().speed(0).speed, 1);
    assert_eq!(Config::default().speed(99).speed, 10);
    assert_eq!(Config::default().speed(5).speed, 5);
}

#[test]
fn config_is_clone() {
    let a = Config::default().quality(42);
    let b = a.clone();
    assert_eq!(a.quality, b.quality);
}

#[test]
fn converter_config_accessor() {
    let cfg = Config::default().quality(77);
    let converter = Converter::new(cfg).unwrap();
    assert_eq!(converter.config().quality, 77);
}
