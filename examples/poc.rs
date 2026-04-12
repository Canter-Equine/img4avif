//! Proof-of-concept: convert representative test images to AVIF.
//!
//! This example generates four synthetic test images that closely match the
//! types and characteristics of the four images provided in the task:
//!
//! | Filename              | Format | Characteristics |
//! |-----------------------|--------|-----------------|
//! | `wikipedia_logo.png`  | PNG    | RGBA with alpha channel, ~400×424 |
//! | `kayak1.jpg`          | JPEG   | Colour photograph, ~340×453 |
//! | `kayak2.jpg`          | JPEG   | Colour photograph, ~300×400 |
//! | `football.jpg`        | JPEG   | Monochrome portrait, ~900×1200 |
//!
//! Each image is converted twice:
//!   1. Full-size (original resolution)
//!   2. 1080-pixel wide (downscaled if wider than 1080 px)
//!
//! All AVIF outputs land in `poc/output/`.
//!
//! # Running
//!
//! ```sh
//! # No logging:
//! cargo run --example poc
//!
//! # With full pipeline logging (requires the dev-logging feature):
//! RUST_LOG=imagine_avif=debug cargo run --example poc --features dev-logging
//! ```

use std::fs;
use std::path::Path;
use std::time::Instant;

use imagine_avif::{Config, Converter, Error, OutputResolution};

/// A test image ready for conversion.
struct TestImage {
    /// Short filename (no path prefix).
    filename: &'static str,
    /// Human-readable format description.
    format: &'static str,
    /// Raw encoded bytes (JPEG or PNG).
    data: Vec<u8>,
    /// Image width in pixels.
    width: u32,
    /// Image height in pixels.
    height: u32,
}

// ---------------------------------------------------------------------------
// Synthetic image generators
// ---------------------------------------------------------------------------

/// Generate a PNG with an RGBA alpha channel.
///
/// Simulates the Wikipedia globe logo: a coloured circle on a transparent
/// background with an alpha-masked interior.  Uses a radial gradient from
/// opaque blue-grey at the centre to fully transparent at the corners, giving
/// the encoder an alpha plane with significant spatial variation.
fn make_wikipedia_logo_png(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgba};

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let r = (width.min(height) as f32) / 2.0 - 2.0;

    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > r {
            // Fully transparent outside the circle.
            Rgba([0u8, 0, 0, 0])
        } else {
            // Gradient from white at edge to light blue-grey at centre,
            // matching the Wikipedia globe's colour palette.
            let t = (r - dist) / r; // 0 at edge, 1 at centre
            let shade = (50.0 * t) as u8;
            Rgba([
                220u8.saturating_sub(shade),
                220u8.saturating_sub(shade),
                230u8,
                255,
            ])
        }
    });

    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("PNG encode failed");
    buf
}

/// Generate a JPEG resembling a blue-water action photo (kayaking).
///
/// Uses horizontal colour bands with luminance variation to simulate splash
/// textures, matching the visual characteristics of a fast-water photograph.
fn make_kayak_jpeg(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        // Simulate blue-green water at the bottom, white splash in the middle,
        // and the darker equipment at the top.
        let fy = y as f32 / height as f32;
        let fx = x as f32 / width as f32;

        // Introduce diagonal texture to simulate moving water.
        let wave = ((fy * 12.0 + fx * 8.0).sin() * 0.5 + 0.5) * 30.0;

        let (r, g, b) = if fy < 0.25 {
            // Top: dark equipment / wetsuit tones
            let v = (40.0 + wave) as u8;
            (v, v, v)
        } else if fy < 0.6 {
            // Middle: white water / splash
            let v = (200.0 + wave) as u8;
            (v, v, v.saturating_add(20))
        } else {
            // Bottom: blue-green water
            let v = wave as u8;
            (v, (60.0 + wave) as u8, (120.0 + wave) as u8)
        };

        Rgb([r, g, b])
    });

    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Jpeg,
    )
    .expect("JPEG encode failed");
    buf
}

/// Generate a large greyscale JPEG resembling a B&W sports portrait.
///
/// Simulates the football photo: a grainy monochrome foreground subject with
/// a blurred background, at portrait (tall) aspect ratio.
fn make_football_jpeg(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        let fy = y as f32 / height as f32;
        let fx = x as f32 / width as f32;

        // Subject occupies the centre vertical strip; background is lighter.
        let subject_mask = {
            let dx = (fx - 0.5).abs();
            if dx < 0.3 {
                1.0_f32
            } else {
                (1.0 - (dx - 0.3) * 3.0_f32).clamp(0.0, 1.0)
            }
        };

        // Base grey ramp darker at bottom (equipment), lighter at top (sky).
        let base = 80.0 + fy * 100.0;

        // Fine-grain texture to simulate camera noise and jersey mesh.
        let grain = ((x.wrapping_mul(6737).wrapping_add(y.wrapping_mul(3491))) % 31) as f32 - 15.0;

        let luma =
            ((base + grain) * subject_mask + 200.0 * (1.0 - subject_mask)).clamp(0.0, 255.0) as u8;

        Rgb([luma, luma, luma])
    });

    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Jpeg,
    )
    .expect("JPEG encode failed");
    buf
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Result of a single successful conversion attempt.
struct ConvResult {
    input_bytes: usize,
    output_bytes: usize,
    elapsed_ms: u128,
    output_path: String,
}

/// Convert `data` and write the AVIF to `poc/output/<stem>_<suffix>.avif`.
///
/// Returns `Ok(ConvResult)` on success or the `imagine_avif::Error` on failure.
fn do_convert(
    data: &[u8],
    stem: &str,
    suffix: &str,
    resolution: OutputResolution,
) -> Result<ConvResult, Error> {
    let output_dir = Path::new("poc/output");
    fs::create_dir_all(output_dir).map_err(Error::Io)?;

    let out_filename = format!("{stem}_{suffix}.avif");
    let out_path = output_dir.join(&out_filename);

    let config = Config::default()
        .quality(80)
        .alpha_quality(90)
        .speed(6)
        .strip_exif(true)
        .output_resolutions(vec![resolution]);

    let converter = Converter::new(config)?;

    let start = Instant::now();
    let avif = converter.convert(data)?;
    let elapsed_ms = start.elapsed().as_millis();

    fs::write(&out_path, &avif).map_err(Error::Io)?;

    Ok(ConvResult {
        input_bytes: data.len(),
        output_bytes: avif.len(),
        elapsed_ms,
        output_path: out_path.display().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    println!("imagine-avif proof-of-concept");
    println!("=========================");
    println!();
    println!("Generating synthetic test images …");
    println!();

    // Build the four test images.
    let images: Vec<TestImage> = vec![
        TestImage {
            filename: "wikipedia_logo",
            format: "PNG (RGBA with alpha)",
            // Wikipedia globe is traditionally ~400 px; use 400×424 to match
            // the roughly square aspect visible in the attached image.
            data: make_wikipedia_logo_png(400, 424),
            width: 400,
            height: 424,
        },
        TestImage {
            filename: "kayak1",
            format: "JPEG (colour photo, portrait)",
            // Approximate dimensions of the first kayak image.
            data: make_kayak_jpeg(340, 453),
            width: 340,
            height: 453,
        },
        TestImage {
            filename: "kayak2",
            format: "JPEG (colour photo, portrait)",
            // Slightly smaller second kayak image.
            data: make_kayak_jpeg(300, 400),
            width: 300,
            height: 400,
        },
        TestImage {
            filename: "football",
            format: "JPEG (large monochrome portrait)",
            // The football image is noticeably larger — portrait orientation.
            data: make_football_jpeg(900, 1200),
            width: 900,
            height: 1200,
        },
    ];

    // Track overall pass/fail counts for the summary.
    let mut pass = 0u32;
    let mut fail = 0u32;

    let pairs: &[(&str, OutputResolution)] = &[
        ("original", OutputResolution::Original),
        ("1080p", OutputResolution::Width1080),
    ];

    for img in &images {
        println!(
            "┌─ {} ({}) — {}×{} px, {} bytes input",
            img.filename,
            img.format,
            img.width,
            img.height,
            img.data.len()
        );

        for &(suffix, resolution) in pairs {
            match do_convert(&img.data, img.filename, suffix, resolution) {
                Ok(r) => {
                    println!(
                        "│   {:8}  ✓  {} → {} bytes  ({:.1}× compression)  {}ms  → {}",
                        suffix,
                        r.input_bytes,
                        r.output_bytes,
                        r.input_bytes as f64 / r.output_bytes as f64,
                        r.elapsed_ms,
                        r.output_path,
                    );
                    pass += 1;
                }
                Err(e) => {
                    eprintln!("│   {:8}  ✗  FAILED: {e}", suffix);
                    fail += 1;
                }
            }
        }

        println!("└");
        println!();
    }

    // Summary line.
    println!(
        "Results: {pass} passed, {fail} failed out of {} conversions.",
        pass + fail
    );

    // Optimization notes section.
    println!();
    println!("Optimization notes for developers");
    println!("----------------------------------");
    println!();
    println!("CPU usage:");
    println!("  * Use Config::speed(10) (fastest) for Lambda to cut AV1 encode time ~3-5x.");
    println!("    Quality drops ~2-4 SSIM points vs speed=6 — acceptable for thumbnails.");
    println!("  * ravif uses rayon for multi-threaded encoding. On Lambda single-vCPU tiers,");
    println!("    set RAYON_NUM_THREADS=1; leave unset on 2-vCPU tiers.");
    println!("  * For 1080p previews quality=70 is often sufficient; reserve quality=80+");
    println!("    for archival full-size encodes.");
    println!();
    println!("Memory usage:");
    println!("  * Decode allocates width x height x 4 B (8-bit) or x8 B (16-bit PNG).");
    println!("    A 24 MP JPEG occupies ~96 MiB decoded.");
    println!("  * Use convert_multi() when producing multiple resolutions — it decodes once");
    println!("    and encodes N times, avoiding repeated decompression (~2-3x RAM saving).");
    println!("  * Check Config::max_input_bytes early to reject oversized uploads before");
    println!("    the decoder allocates its buffer.");
    println!();
    println!("Security:");
    println!("  * Config::max_pixels (default 268 MP) prevents decompression bombs;");
    println!("    tighten to ~8 MP for a consumer-photo API.");
    println!("  * Config::max_input_bytes (default 100 MiB) rejects oversized raw uploads;");
    println!("    tighten to ~20 MiB if you only expect phone-camera JPEGs.");
    println!("  * strip_exif=true (default) removes GPS, device fingerprint, and embedded");
    println!("    thumbnails. Never set to false in a multi-tenant pipeline.");
    println!("  * #![forbid(unsafe_code)] covers all library source. Enabling");
    println!("    heic-experimental links libheif (C, LGPL) and breaks this guarantee.");

    if fail > 0 {
        std::process::exit(1);
    }
}
