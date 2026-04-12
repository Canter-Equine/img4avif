//! Integration tests that exercise real fixture files on disk.
//!
//! - `examples/fixtures/valid/`   – every recognised image (JPEG / PNG / WebP)
//!   **must** convert successfully; outputs land in `examples/out/`.
//! - `examples/fixtures/invalid/` – every file **must** fail to convert.
//!
//! These tests run as part of the normal `cargo test` suite and therefore
//! exercise the CI pipeline at no extra job cost.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use imagine_avif::{Config, Converter};

/// Extensions that `imagine_avif` accepts in its default build.
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

/// Every image file in `examples/fixtures/valid/` must convert to AVIF.
/// Outputs are written to `examples/out/<original-filename>.avif`.
#[test]
fn valid_fixtures_convert_to_avif() {
    let valid_dir = Path::new("examples/fixtures/valid");
    let out_dir = Path::new("examples/out");
    fs::create_dir_all(out_dir).expect("failed to create examples/out/");

    let converter =
        Converter::new(Config::default().quality(60).speed(10)).expect("failed to build Converter");

    let mut checked = 0u32;
    let mut entries: Vec<_> = fs::read_dir(valid_dir)
        .expect("examples/fixtures/valid/ not found")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .collect();
    entries.sort();

    for path in &entries {
        let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("?");
        let data = fs::read(path).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

        let avif = converter
            .convert(&data)
            .unwrap_or_else(|e| panic!("convert {file_name} failed: {e}"));
        assert!(!avif.is_empty(), "{file_name} produced empty AVIF");

        let out_path = out_dir.join(format!("{file_name}.avif"));
        fs::write(&out_path, &avif)
            .unwrap_or_else(|e| panic!("write {} failed: {e}", out_path.display()));

        checked += 1;
    }

    assert!(
        checked > 0,
        "examples/fixtures/valid/ contained no recognised image files"
    );
}

/// Every file in `examples/fixtures/invalid/` must fail to convert.
#[test]
fn invalid_fixtures_all_fail() {
    let invalid_dir = Path::new("examples/fixtures/invalid");

    let converter = Converter::new(Config::default()).expect("failed to build Converter");

    let mut checked = 0u32;
    let mut entries: Vec<_> = fs::read_dir(invalid_dir)
        .expect("examples/fixtures/invalid/ not found")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    for path in &entries {
        let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("?");
        let data = fs::read(path).unwrap_or_else(|e| panic!("read {file_name}: {e}"));

        assert!(
            converter.convert(&data).is_err(),
            "{file_name} unexpectedly converted to AVIF — move it to fixtures/valid/ if intentional"
        );
        checked += 1;
    }

    assert!(checked > 0, "examples/fixtures/invalid/ contained no files");
}
