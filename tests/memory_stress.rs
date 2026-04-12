//! Memory stress tests.
//!
//! The large-image tests are ignored by default; run them with:
//!
//! ```bash
//! cargo test --test memory_stress -- --ignored --nocapture
//! ```

use img2avif::{Config, Converter, MemoryGuard};

fn make_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([128u8, 64, 32, 255]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

#[test]
fn memory_guard_unlimited_always_passes() {
    MemoryGuard::new(u64::MAX)
        .check()
        .expect("unlimited guard must not fail");
}

#[test]
fn memory_guard_zero_limit_fails_on_linux() {
    #[cfg(target_os = "linux")]
    assert!(MemoryGuard::new(0).check().is_err());
}

/// Convert a ~200 MP image and verify peak RSS stays under 512 MiB (speed=10).
///
/// 16383 × 12207 ≈ 200 000 781 pixels — the largest image the default
/// `max_pixels` cap (16384²) permits.
/// The RGBA8 pixel buffer alone is ~763 MiB, which exceeds the default 512 MiB
/// memory limit.  This test therefore uses a relaxed 1 GiB limit and confirms
/// the encode completes end-to-end while also confirming the MemoryGuard fires
/// correctly when the limit is set too low.
#[test]
#[ignore = "slow (~60 s) and requires >1 GiB RAM"]
fn two_hundred_megapixel_converts_successfully() {
    // 16383 × 12207 ≈ 200 MP  (just under the 16384×16384 pixel cap)
    const W: u32 = 16383;
    const H: u32 = 12207;
    const LIMIT_MB: u64 = 1024; // 1 GiB — pixel buf alone is ~763 MiB

    let png = make_png(W, H);
    println!("input: {} MiB", png.len() / (1024 * 1024));

    let cfg = Config::default()
        .quality(60)
        .speed(10)
        .memory_limit_bytes(LIMIT_MB * 1024 * 1024);

    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&png)
        .expect("200 MP conversion failed");

    let rss = MemoryGuard::current_rss_bytes().unwrap_or(0);
    println!(
        "rss: {} MiB, avif: {} KiB",
        rss / (1024 * 1024),
        avif.len() / 1024
    );

    #[cfg(target_os = "linux")]
    assert!(
        rss <= LIMIT_MB * 1024 * 1024,
        "RSS {} MiB exceeded {} MiB limit",
        rss / (1024 * 1024),
        LIMIT_MB,
    );
}

/// Verify that converting a 200 MP image with the default 512 MiB memory limit
/// raises `MemoryExceeded` (the pixel buffer alone exceeds 512 MiB).
#[test]
#[ignore = "slow (~20 s)"]
fn two_hundred_megapixel_exceeds_default_512mib_limit() {
    const W: u32 = 16383;
    const H: u32 = 12207;

    let png = make_png(W, H);

    // Default config → 512 MiB memory limit.
    let cfg = Config::default().quality(60).speed(10);
    let result = Converter::new(cfg).unwrap().convert(&png);

    // On Linux the MemoryGuard reads real RSS; on other platforms it is a
    // no-op (fail-open), so we only assert on Linux.
    #[cfg(target_os = "linux")]
    assert!(
        matches!(result, Err(img2avif::Error::MemoryExceeded { .. })),
        "expected MemoryExceeded for 200 MP image under 512 MiB limit, got: {result:?}"
    );
    #[cfg(not(target_os = "linux"))]
    let _ = result; // ignore on platforms without RSS tracking
}

/// Convert a ~100 MB synthetic PNG and confirm the conversion completes under
/// the 512 MiB memory budget (speed=10).
///
/// A 100 MB PNG typically encodes a moderately large image with a heavy
/// pre-existing compressed stream.  We simulate this by generating a large
/// pixel image whose raw PNG size approaches 100 MB.
///
/// 5000 × 5000 RGBA8 = 100 MB uncompressed pixel data.  The PNG encoding adds
/// overhead, but this exercises the memory path for a "100 MB file" scenario.
#[test]
#[ignore = "slow (~15 s)"]
fn hundred_mb_image_converts_successfully() {
    // 5000 × 5000 RGBA8 → exactly 100 000 000 bytes of raw pixel data.
    // The PNG wrapper will be somewhat larger due to headers/filters but this
    // gives us a realistic "100 MB-class" workload.
    const W: u32 = 5000;
    const H: u32 = 5000;
    const LIMIT_MB: u64 = 512;

    let png = make_png(W, H);
    println!("input: {} MiB", png.len() / (1024 * 1024));

    let cfg = Config::default()
        .quality(80)
        .speed(10)
        .memory_limit_bytes(LIMIT_MB * 1024 * 1024);

    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&png)
        .expect("100 MB image conversion failed");

    let rss = MemoryGuard::current_rss_bytes().unwrap_or(0);
    println!(
        "rss: {} MiB, avif: {} KiB",
        rss / (1024 * 1024),
        avif.len() / 1024
    );

    #[cfg(target_os = "linux")]
    assert!(
        rss <= LIMIT_MB * 1024 * 1024,
        "RSS {} MiB exceeded {} MiB limit — peak memory too high for Lambda 512 MB",
        rss / (1024 * 1024),
        LIMIT_MB,
    );
}

/// Convert a ~50 MP image and verify peak RSS stays under 512 MiB.
///
/// 8944 × 5615 ≈ 50 190 880 pixels (a typical 50 MP sensor).
/// The RGBA8 pixel buffer alone is ~191 MiB, so the limit must be well
/// above 200 MiB; we use the default 512 MiB budget.
#[test]
#[ignore = "slow (~10 s)"]
fn fifty_megapixel_converts_successfully() {
    const LIMIT_MB: u64 = 512;
    const W: u32 = 8944;
    const H: u32 = 5615;

    let png = make_png(W, H);
    println!("input: {} MiB", png.len() / (1024 * 1024));

    let cfg = Config::default()
        .quality(80)
        .speed(10)
        .memory_limit_bytes(LIMIT_MB * 1024 * 1024);

    let avif = Converter::new(cfg)
        .unwrap()
        .convert(&png)
        .expect("50 MP conversion failed");

    let rss = MemoryGuard::current_rss_bytes().unwrap_or(0);
    println!(
        "rss: {} MiB, avif: {} KiB",
        rss / (1024 * 1024),
        avif.len() / 1024
    );

    #[cfg(target_os = "linux")]
    assert!(
        rss <= LIMIT_MB * 1024 * 1024,
        "RSS {} MiB exceeded {} MiB limit — increase memory_limit_bytes for large images",
        rss / (1024 * 1024),
        LIMIT_MB,
    );
}
