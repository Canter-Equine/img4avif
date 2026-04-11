//! Convert every file in the `examples/` directory to AVIF.
//!
//! # Behaviour
//!
//! - **Valid image inputs** (JPEG / PNG / WebP): decoded and re-encoded as
//!   AVIF; the output is written to `examples/out/<stem>.avif`.
//! - **Unsupported but structurally valid containers** (HEIC without the
//!   `heic-experimental` feature, or AVIF passed as input): rejected early
//!   with `Error::UnsupportedFormat` — no heavy pixel allocation occurs.
//! - **Non-image files** (`.rs`, `.mhtml`, binary blobs …): rejected at the
//!   format-detection stage with `Error::Decode` — the decoder reads only the
//!   magic bytes before returning the error.
//!
//! # Running
//!
//! ```sh
//! cargo run --example convert_examples
//! ```
//!
//! Outputs land in `examples/out/`.  The process exits with code 0 when every
//! *image* file converts successfully; code 1 on any unexpected conversion
//! failure.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use img2avif::{Config, Converter, Error};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// File extensions we recognise as valid image inputs for img2avif **in
/// the default build** (no optional feature flags).  HEIC/HEIF requires the
/// `heic-experimental` feature and is therefore excluded here — those files
/// are expected to fail with `UnsupportedFormat`, which is correct behaviour.
const VALID_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Whether a path's extension is in the known-image list (case-insensitive).
fn is_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|e| {
            VALID_IMAGE_EXTENSIONS
                .iter()
                .any(|&valid| e.eq_ignore_ascii_case(valid))
        })
        .unwrap_or(false)
}

/// Collect all *files* (not directories) directly inside `dir`, sorted by
/// file name for deterministic output order.
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .expect("examples/ directory not found")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            // Skip subdirectories (e.g. examples/out/).
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let examples_dir = Path::new("examples");
    let out_dir = examples_dir.join("out");

    // Create the output directory if it does not exist.
    fs::create_dir_all(&out_dir).expect("failed to create examples/out/");

    let files = collect_files(examples_dir);

    if files.is_empty() {
        println!("No files found in examples/");
        return;
    }

    // Build a single converter to reuse across all images.
    let config = Config::default()
        .quality(80)
        .alpha_quality(90)
        .speed(6)
        .strip_exif(true);
    let converter = Converter::new(config).expect("failed to build Converter");

    println!("img2avif — batch conversion of examples/");
    println!("=========================================");
    println!();
    println!("Output directory: {}", out_dir.display());
    println!();

    let mut succeeded: Vec<&Path> = Vec::new();
    let mut expected_failures: Vec<(&Path, String)> = Vec::new();
    let mut unexpected_failures: Vec<(&Path, String)> = Vec::new();

    for path in &files {
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("<unknown>");

        // Read the raw bytes first.
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                unexpected_failures.push((path.as_path(), format!("I/O error: {e}")));
                println!("  ✗  {file_name}  →  I/O error: {e}");
                continue;
            }
        };

        let is_image = is_image_extension(path);

        print!("  {file_name}  ({} bytes)  ", data.len());

        let start = Instant::now();
        let result = converter.convert(&data);
        let elapsed_ms = start.elapsed().as_millis();

        match result {
            Ok(avif) => {
                // Write the AVIF output.  Use the full original filename
                // (including its extension) as the stem so that files with
                // the same base name but different formats (e.g. `2.sm.jpg`
                // and `2.sm.webp`) produce distinct outputs.
                let out_name = format!("{file_name}.avif");
                let out_path = out_dir.join(&out_name);
                match fs::write(&out_path, &avif) {
                    Ok(()) => {
                        println!(
                            "→  ✓  {} bytes AVIF  ({:.1}× compression)  {elapsed_ms}ms  → {}",
                            avif.len(),
                            data.len() as f64 / avif.len() as f64,
                            out_path.display(),
                        );
                        succeeded.push(path.as_path());
                    }
                    Err(e) => {
                        println!("→  ✗  conversion succeeded but write failed: {e}");
                        unexpected_failures.push((path.as_path(), format!("write failed: {e}")));
                    }
                }
            }
            Err(ref e) => {
                let reason = e.to_string();
                if is_image {
                    // An image extension that we expected to succeed.
                    println!("→  ✗  UNEXPECTED failure ({elapsed_ms}ms): {e}");
                    unexpected_failures.push((path.as_path(), reason));
                } else {
                    // Not a recognised image extension — failing is the correct outcome.
                    let kind = match e {
                        Error::Decode(_) => "Decode",
                        Error::UnsupportedFormat(_) => "UnsupportedFormat",
                        _ => "Error",
                    };
                    println!("→  ✓  expected failure ({kind}, {elapsed_ms}ms): {e}");
                    expected_failures.push((path.as_path(), reason));
                }
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Summary
    // ---------------------------------------------------------------------------
    println!();
    println!("Summary");
    println!("-------");
    println!("  Successful conversions : {}", succeeded.len());
    println!(
        "  Expected failures      : {}  (non-image inputs rejected correctly)",
        expected_failures.len()
    );

    if !unexpected_failures.is_empty() {
        println!(
            "  Unexpected failures    : {}  ← PROBLEM",
            unexpected_failures.len()
        );
        for (p, e) in &unexpected_failures {
            println!("    - {}  →  {e}", p.display());
        }
        println!();
        eprintln!(
            "ERROR: {} file(s) failed that should have converted successfully.",
            unexpected_failures.len()
        );
        std::process::exit(1);
    } else {
        println!("  Unexpected failures    : 0");
        println!();
        println!("All image files converted successfully.");
    }
}
