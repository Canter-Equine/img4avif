//! Integration tests that exercise real fixture files on disk.
//!
//! - `examples/fixtures/invalid/` – every file **must** fail to convert.
//!
//! Valid-fixture conversion is covered by `fixture_timing`, which runs the
//! full fixture set with timing and RSS reporting on every CI run.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use img4avif::{Config, Converter};

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
