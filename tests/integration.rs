//! End-to-end conversion tests.  All images are generated in-memory.

use img2avif::{Config, Converter, Error};

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
    assert_eq!(Config::default().quality(200).quality, 100);
}

#[test]
fn config_speed_clamped() {
    assert_eq!(Config::default().speed(0).speed, 1);
    assert_eq!(Config::default().speed(99).speed, 10);
}

#[test]
fn config_is_clone() {
    let a = Config::default().quality(42);
    assert_eq!(a.clone().quality, 42);
}

#[test]
fn converter_exposes_config() {
    let cfg = Config::default().quality(77);
    assert_eq!(Converter::new(cfg).unwrap().config().quality, 77);
}
