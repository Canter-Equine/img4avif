//! Memory stress tests.
//!
//! The 50 MP test is ignored by default; run it with:
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
