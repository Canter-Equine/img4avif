//! Memory stress tests.
//!
//! The heavy 50 MP test is `#[ignore]`d by default to keep normal `cargo test`
//! fast.  Run it with:
//!
//! ```bash
//! cargo test --test memory_stress -- --ignored --nocapture
//! ```

use img2avif::{Config, Converter, Error, MemoryGuard};

const LIMIT_MB: u64 = 150;

/// Generate a solid-colour PNG to avoid a huge random allocation.
fn make_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([128u8, 64, 32, 255]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

// ── Always-run smoke test ─────────────────────────────────────────────────

#[test]
fn memory_guard_unlimited_always_passes() {
    let guard = MemoryGuard::new(u64::MAX);
    guard.check().expect("unlimited guard should never fail");
}

#[test]
fn memory_guard_zero_limit_fails_on_linux() {
    #[cfg(target_os = "linux")]
    {
        let guard = MemoryGuard::new(0);
        assert!(
            guard.check().is_err(),
            "zero-byte limit must be exceeded on Linux"
        );
    }
}

// ── Slow stress test (ignored by default) ─────────────────────────────────

/// Convert a synthetic ~50 MP image and verify peak RSS stays below 150 MB.
///
/// 8944 × 5615 ≈ 50 190 880 pixels, matching a typical 50 MP camera sensor.
#[test]
#[ignore = "slow (~10 s); run with `cargo test -- --ignored`"]
fn fifty_megapixel_under_150mb() {
    const W: u32 = 8944;
    const H: u32 = 5615;

    let png = make_png(W, H);
    println!("Input PNG: {} MB", png.len() / (1024 * 1024));

    let rss_before = MemoryGuard::current_rss_bytes().unwrap_or(0);
    println!("RSS before: {} MB", rss_before / (1024 * 1024));

    let config = Config::default()
        .quality(80)
        .speed(10) // fastest encoder for the stress test
        .memory_limit_bytes(LIMIT_MB * 1024 * 1024);

    let converter = Converter::new(config).unwrap();

    match converter.convert(&png) {
        Ok(avif) => {
            let rss_after = MemoryGuard::current_rss_bytes().unwrap_or(0);
            println!("RSS after:  {} MB", rss_after / (1024 * 1024));
            println!("AVIF output: {} KB", avif.len() / 1024);

            #[cfg(target_os = "linux")]
            assert!(
                rss_after <= LIMIT_MB * 1024 * 1024,
                "peak RSS {rss_after} bytes exceeded {LIMIT_MB} MB limit"
            );
        }
        Err(Error::MemoryExceeded { used_mb, limit_mb }) => {
            panic!("Memory guard triggered: {used_mb} MB > {limit_mb} MB limit");
        }
        Err(e) => panic!("Unexpected error: {e}"),
    }
}
