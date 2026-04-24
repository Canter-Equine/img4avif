//! WASM fixture conversion tests with specific quality/speed settings.
//!
//! These tests are run as part of the `wasm-mobile` CI job to verify that
//! real fixture files convert successfully with non-default encoder settings,
//! and to upload the resulting AVIF files as CI artifacts.
//!
//! Run standalone with visible output:
//!
//! ```sh
//! cargo test --test wasm_fixture_conversion -- --nocapture
//! ```

use std::fs;
use std::path::Path;
use std::time::Instant;

use img4avif::{Config, Converter, MemoryGuard};

/// Convert `examples/fixtures/valid/Horse Jumping.png` with quality=10, speed=10.
///
/// Output is written to `examples/out/Horse Jumping.png.wasm-10.avif` and uploaded
/// as the `fixture-conversion-wasm-10` CI artifact.
#[test]
fn wasm_fixture_quality10_speed10() {
    let input_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/valid/Horse Jumping.png");
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/out");
    fs::create_dir_all(&out_dir).expect("failed to create examples/out/");

    let file_name = "Horse Jumping.png";

    let data = fs::read(&input_path).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

    let config = Config::default().quality(10).speed(10);
    let converter = Converter::new(config).expect("failed to build Converter");

    println!();
    println!("img4avif wasm fixture conversion  (speed=10, quality=10)");
    println!("{}", "=".repeat(72));

    let rss_before = MemoryGuard::current_rss_bytes().unwrap_or(0);
    let start = Instant::now();
    let avif = converter
        .convert(&data)
        .unwrap_or_else(|e| panic!("convert {file_name} failed: {e}"));
    let elapsed_ms = start.elapsed().as_millis();
    let rss_after = MemoryGuard::current_rss_bytes().unwrap_or(0);
    let rss_delta_mb = rss_after.saturating_sub(rss_before) / (1024 * 1024);
    let avif_kb = avif.len() / 1024;

    println!(
        "{:<46} {:>8} ms  rss_Δ={} MB  avif={} KB",
        file_name, elapsed_ms, rss_delta_mb, avif_kb
    );

    let out_path = out_dir.join(format!("{file_name}.wasm-10.avif"));
    fs::write(&out_path, &avif)
        .unwrap_or_else(|e| panic!("write {} failed: {e}", out_path.display()));

    println!("{}", "-".repeat(72));
    println!();
}

/// Convert `examples/fixtures/valid/Horse Jumping.png` with quality=5, speed=5.
///
/// Output is written to `examples/out/Horse Jumping.png.wasm-5.avif`
/// and uploaded as the `fixture-conversion-wasm-5` CI artifact.
#[test]
fn wasm_fixture_quality5_speed5() {
    let input_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/valid/Horse Jumping.png");
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/out");
    fs::create_dir_all(&out_dir).expect("failed to create examples/out/");

    let file_name = "Horse Jumping.png";

    let data = fs::read(&input_path).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

    let config = Config::default().quality(5).speed(5);
    let converter = Converter::new(config).expect("failed to build Converter");

    println!();
    println!("img4avif wasm fixture conversion  (speed=5, quality=5)");
    println!("{}", "=".repeat(72));

    let rss_before = MemoryGuard::current_rss_bytes().unwrap_or(0);
    let start = Instant::now();
    let avif = converter
        .convert(&data)
        .unwrap_or_else(|e| panic!("convert {file_name} failed: {e}"));
    let elapsed_ms = start.elapsed().as_millis();
    let rss_after = MemoryGuard::current_rss_bytes().unwrap_or(0);
    let rss_delta_mb = rss_after.saturating_sub(rss_before) / (1024 * 1024);
    let avif_kb = avif.len() / 1024;

    println!(
        "{:<46} {:>8} ms  rss_Δ={} MB  avif={} KB",
        file_name, elapsed_ms, rss_delta_mb, avif_kb
    );

    let out_path = out_dir.join(format!("{file_name}.wasm-5.avif"));
    fs::write(&out_path, &avif)
        .unwrap_or_else(|e| panic!("write {} failed: {e}", out_path.display()));

    println!("{}", "-".repeat(72));
    println!();
}
