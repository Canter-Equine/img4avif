//! Conversion timing tests using real fixture files from `examples/fixtures/valid/`.
//!
//! Each recognised image is converted with **speed = 6, quality = 80** (the
//! same settings as [`convert_examples`](../examples/convert_examples.rs)).
//! Elapsed wall-clock time and the change in process RSS are printed to
//! standard output for every file so that CI logs provide an auditable
//! performance baseline.
//!
//! Run standalone with visible output:
//!
//! ```sh
//! cargo test --test fixture_timing -- --nocapture
//! ```

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::Instant;

use img4avif::{Config, Converter, MemoryGuard};

/// Extensions that `img4avif` accepts in its default build.
/// HEIC/HEIF requires the `heic-experimental` feature and is excluded.
const VALID_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|e| {
            VALID_IMAGE_EXTENSIONS
                .iter()
                .any(|&v| e.eq_ignore_ascii_case(v))
        })
        .unwrap_or(false)
}

/// Convert every image in `examples/fixtures/valid/` using speed=6, quality=80
/// and print per-file timing and RSS allocation to stdout.
#[test]
fn valid_fixtures_timed_conversion() {
    let valid_dir = Path::new("examples/fixtures/valid");
    let out_dir = Path::new("examples/out");
    fs::create_dir_all(out_dir).expect("failed to create examples/out/");

    let config = Config::default().quality(80).speed(6);
    let converter = Converter::new(config).expect("failed to build Converter");

    let mut entries: Vec<_> = fs::read_dir(valid_dir)
        .expect("examples/fixtures/valid/ not found")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .collect();
    entries.sort();

    println!();
    println!("img4avif fixture conversion timing  (speed=6, quality=80)");
    println!("{}", "=".repeat(72));
    println!(
        "{:<46} {:>8}  {:>9}  {:>9}",
        "file", "ms", "rss_Δ_MB", "avif_KB"
    );
    println!("{}", "-".repeat(72));

    let mut any_failed = false;
    for path in &entries {
        let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("?");
        let data = fs::read(path).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

        let rss_before = MemoryGuard::current_rss_bytes().unwrap_or(0);
        let start = Instant::now();
        let result = converter.convert(&data);
        let elapsed_ms = start.elapsed().as_millis();
        let rss_after = MemoryGuard::current_rss_bytes().unwrap_or(0);

        // Report the RSS delta.  Saturating sub avoids underflow when RSS
        // decreases between measurements (e.g. OS reclaims pages).
        let rss_delta_mb = rss_after.saturating_sub(rss_before) / (1024 * 1024);

        match result {
            Ok(avif) => {
                let avif_kb = avif.len() / 1024;
                println!(
                    "{:<46} {:>8}  {:>9}  {:>9}",
                    file_name, elapsed_ms, rss_delta_mb, avif_kb
                );
                let out_path = out_dir.join(format!("{file_name}.avif"));
                fs::write(&out_path, &avif)
                    .unwrap_or_else(|e| panic!("write {} failed: {e}", out_path.display()));
            }
            Err(e) => {
                println!("{file_name}  FAILED: {e}");
                any_failed = true;
            }
        }
    }

    println!("{}", "-".repeat(72));
    println!();

    assert!(
        !any_failed,
        "one or more fixture conversions failed — check output above"
    );
}

/// Convert only the smallest valid image in `examples/fixtures/valid/`.
///
/// Used by the macOS and Windows CI matrix jobs where we only need to confirm
/// the code compiles and can successfully convert at least one real file
/// without running the full (potentially slow) fixture suite.
#[test]
fn smallest_fixture_timed_conversion() {
    let valid_dir = Path::new("examples/fixtures/valid");
    let out_dir = Path::new("examples/out");
    fs::create_dir_all(out_dir).expect("failed to create examples/out/");

    let config = Config::default().quality(80).speed(6);
    let converter = Converter::new(config).expect("failed to build Converter");

    let smallest = fs::read_dir(valid_dir)
        .expect("examples/fixtures/valid/ not found")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .min_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX))
        .expect("no valid image fixtures found");

    let file_name = smallest
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("?");
    let data = fs::read(&smallest).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

    println!();
    println!("img4avif smallest-fixture timing  (speed=6, quality=80)");
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

    let out_path = out_dir.join(format!("{file_name}.avif"));
    fs::write(&out_path, &avif)
        .unwrap_or_else(|e| panic!("write {} failed: {e}", out_path.display()));

    println!("{}", "-".repeat(72));
    println!();
}
