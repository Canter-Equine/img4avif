# img4avif

[![Crates.io](https://img.shields.io/crates/v/img4avif.svg)](https://crates.io/crates/img4avif)
[![docs.rs](https://docs.rs/img4avif/badge.svg)](https://docs.rs/img4avif)
[![CI](https://github.com/Canter-Equine/img4avif/actions/workflows/ci.yml/badge.svg)](https://github.com/Canter-Equine/img4avif/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://blog.rust-lang.org/2025/01/23/Rust-1.85.0.html)

A fast, memory-safe Rust library that converts **JPEG, PNG, WebP, and HEIC/HEIF**
images to **AVIF** using the pure-Rust `rav1e` AV1 encoder. It also supports
16-bit PNG input for HDR10-style images.


Optimized for **cost-sensitive, high-volume serverless workloads**
on AWS Lambda (Linux x86_64 / aarch64).

---

## Table of contents

1. [Installation](#installation)
2. [Quick start](#quick-start)
3. [Supported input formats](#supported-input-formats)
4. [HDR10 support](#hdr10-support)
5. [Configuration reference](#configuration-reference)
6. [Output resolution control](#output-resolution-control)
7. [EXIF / metadata handling](#exif--metadata-handling)
8. [Memory guard](#memory-guard)
9. [Feature flags](#feature-flags)
10. [Performance benchmarks](#performance-benchmarks)
11. [AWS Lambda deployment](#aws-lambda-deployment)
12. [Security](#security)
13. [License](#license)

---

## Installation

```toml
[dependencies]
img4avif = "0.5"
```

### Minimum supported Rust version (MSRV)

`img4avif` requires **Rust 1.85** or later.  The MSRV is enforced in
`Cargo.toml` and tested in CI.

---

## Quick Start

```rust
use img4avif::{Config, Converter};

fn main() -> Result<(), img4avif::Error> {
    let jpeg_bytes = std::fs::read("photo.jpg")?;

    // Build a config — all fields have sensible defaults.
    let config = Config::default()
        .quality(85)   // 1–100, default 80
        .speed(6)      // 1–10,  default 6
        .strip_exif(true); // default is already true

    let converter = Converter::new(config)?;
    let avif_bytes = converter.convert(&jpeg_bytes)?;

    std::fs::write("photo.avif", &avif_bytes)?;
    Ok(())
}
```

### Lambda Cost Optimised Preset

```rust
use img4avif::{Config, Converter};

let converter = Converter::new(Config::lambda_cost_optimized())?;
// quality=75, speed=10, strip_exif=true, max_input_bytes=50 MiB
let avif = converter.convert(&input_bytes)?;
```

---

## Supported Input Formats

| Format | Extensions | Feature flag | AVIF bit-depth |
|--------|-----------|-------------|---------------|
| JPEG | `.jpg`, `.jpeg` | *(always on)* | 10-bit (ravif auto) |
| PNG (8-bit) | `.png` | *(always on)* | 10-bit (ravif auto) |
| PNG (16-bit / HDR10) | `.png` | *(always on)* | **10-bit** via `encode_raw_planes_10_bit` |
| WebP | `.webp` | *(always on)* | 10-bit (ravif auto) |
| HEIC / HEIF | `.heic`, `.heif` | `heic-experimental` | 10-bit (ravif auto) |

Format detection uses magic bytes, so file extensions are not trusted.

---

## HDR10 support

### 16-bit PNG inputs

16-bit PNG files (the standard delivery format for HDR10 still images) are
decoded at full precision and converted to10-bit AVIF using
`encode_raw_planes_10_bit` to preserve more detail than 8-but ouput

### HEIC with HDR10 metadata

Many smartphone cameras produce HDR10-tagged HEIC files.  Enable the
`heic-experimental` Cargo feature to decode these:

```toml
[dependencies]
img4avif = { version = "0.5", features = ["heic-experimental"] }
```

> ⚠️  Requires `libheif` installed on the system at link time.  See
> [Feature flags](#feature-flags) for details and licensing implications.

---

## Configuration Reference

The `Config` builder lets you balance **image quality**, **file size**, and
**encode speed**:

### Recommended starting points

- **Thumbnails / Lambda / high-throughput pipelines:** use `quality=70`
  and `speed=10`
- **Archival photos / maximum fidelity:** use `quality=95` and a lower
  speed such as `6`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `quality` | `u8` | `80` | Colour encoding quality (1 – 100). Higher value preserves the image quality, lower value produces smaller file size. |
| `alpha_quality` | `u8` | `80` | Alpha-channel quality (1 – 100) preserves visual transparency. Higher value keep the original transparency level, lower value produces smaller file size. |
| `speed` | `u8` | `6` | Encoder speed (1 – 10). Higher value encodes faster, lower value produces smaller file size. |
| `strip_exif` | `bool` | `true` | Strip all EXIF/IPTC/XMP metadata (recommended). |
| `max_input_bytes` | `u64` | `104_857_600` (100 MiB) | Maximum raw input file size. |
| `max_pixels` | `u64` | `268_435_456` (268 MP) | Max pixel count (width × height). |
| `memory_limit_bytes` | `u64` | `536_870_912` (512 MiB) | Peak memory budget for conversion. |
| `output_resolutions` | `Vec<OutputResolution>` | `[Original]` | Which resolution(s) to produce. See [Output resolution control](#output-resolution-control). |

All setter methods return `Self` for chaining:

```rust
let config = Config::default()
    .quality(90)
    .alpha_quality(95)  // keep alpha visually lossless
    .speed(8)
    .max_pixels(10_000 * 10_000)
    .memory_limit_bytes(512 * 1024 * 1024);
```

---

## Output Resolution Control

By default `img4avif` encodes images at their original resolution.  Use
`Config::output_resolutions` with any combination of `OutputResolution`
variants to resize before encoding.

| Variant | Target width | Behaviour |
|---------|-------------|-----------|
| `Original` | — | No resize; encodes at source dimensions |
| `Width2560` | 2560 px | Shrinks to 2560 px wide if source is wider |
| `Width1080` | 1080 px | Shrinks to 1080 px wide if source is wider |

**Rules:**
- **Only downscales.** Images already at or below the target width are
  passed through unchanged — `img4avif` never upscales.
- **Aspect ratio preserved.** Height is computed proportionally; no cropping.
- **Lanczos-3 filter** is used for high-quality downsampling.

### Single output at a specific width

```rust
use img4avif::{Config, Converter, OutputResolution};

let config = Config::default()
    .output_resolutions(vec![OutputResolution::Width1080]);
let avif = Converter::new(config)?.convert(&src_bytes)?;
```

### Multiple outputs in one decode pass

Use `convert_multi` to decode once and get all requested sizes:

```rust
use img4avif::{Config, Converter, ConversionOutput, OutputResolution};

let config = Config::default().output_resolutions(vec![
    OutputResolution::Original,   // full resolution
    OutputResolution::Width2560,  // 2K variant
    OutputResolution::Width1080,  // 1080 p variant
]);

let outputs: Vec<ConversionOutput> = Converter::new(config)?.convert_multi(&src_bytes)?;
for out in &outputs {
    println!("{:?}: {} bytes", out.resolution, out.data.len());
}
```

> **Lambda tip:** `convert_multi` with all three resolutions costs only
> slightly more than a single `convert` call because the decode step runs
> only once.  The three encode passes are independent and parallelisable.

---

## EXIF / metadata handling

By default, img4avif removes all metadata.

If you want to keep metadata, set `strip_exif(false)`:
`:

A warning will be printed to `stderr` at conversion time when `strip_exif = false`.

---

## Memory Guard

The [`MemoryGuard`] checks RSS before and after decoding.  If peak RSS
exceeds `memory_limit_bytes` (default **512 MiB**) conversion is aborted with
[`Error::MemoryExceeded`].

The 512 MiB default comfortably handles images up to ~25 MP RGBA8 on a 512 MB
Lambda (pixel buffer ~96 MiB plus encoder working memory).  For larger images,
raise the limit accordingly or configure a higher-memory Lambda:

| Image size | Min recommended `memory_limit_bytes` |
|-----------|--------------------------------------|
| ≤ 25 MP  | 512 MiB (default)                    |
| ≤ 50 MP  | 512 MiB (default)                    |
| ≤ 100 MP | 768 MiB                              |
| ≤ 200 MP | 1024 MiB                             |

```rust
use img4avif::{Config, Error};

match converter.convert(&huge_image) {
    Err(Error::MemoryExceeded { used_mb, limit_mb }) => {
        eprintln!("Aborted: {used_mb} MB > {limit_mb} MB limit");
    }
    Err(Error::InputTooLarge { width, height, .. }) => {
        eprintln!("Image {width}×{height} exceeds pixel limit");
    }
    Ok(avif) => { /* … */ }
    Err(e) => eprintln!("Error: {e}"),
}
```

| Platform | Memory source       |
|----------|---------------------|
| Linux    | `/proc/self/status` |
| macOS    | `vm_stat` output    |
| Windows  | Not available (fail-open) |

---

## Feature flags

| Flag | Default | Description |
|------|---------|-------------|
| `dev-logging` | **off** | Structured pipeline logging via the [`log`](https://docs.rs/log) crate. Zero overhead when disabled. |
| `heic-experimental` | **off** | HEIC/HEIF decoding via `libheif-rs`. **Requires the `libheif` C library at link time.** |
| `raw-experimental` | **off** | Camera RAW decoding via `rawloader` (pure Rust, unstable API). |

### `dev-logging`

When enabled, `img4avif` emits structured log records under the `img4avif`
target at every pipeline stage.  Use any [`log`-compatible
subscriber](https://docs.rs/log#available-logging-implementations):

```toml
[dependencies]
img4avif = { version = "0.5", features = ["dev-logging"] }
env_logger = "0.11"
```

```rust
// Initialise the subscriber in your binary or test harness:
env_logger::init();
// Then run with: RUST_LOG=img4avif=debug cargo run
```

| Level | What you see |
|-------|-------------|
| `ERROR` | Every error path — context logged before `Err(…)` is returned |
| `WARN` | Non-fatal issues (metadata preservation, suspiciously small output) |
| `INFO` | Per-image milestones: dimensions, pixel format, compression ratio |
| `DEBUG` | Sub-step detail: quality / speed settings, RSS readings, byte counts |

When `dev-logging` is **disabled** (the default), all log macro calls expand
to `()` — the compiler removes them entirely, so there is **zero runtime cost**.

> ⚠️  **HEIC / RAW support is experimental and opt-in.**  The pure-Rust HEIC
> ecosystem is not yet production-ready (as of Rust 1.70 / April 2024).  The
> `heic-experimental` flag introduces a C dependency unsuitable for stock
> Lambda layers.
>
> ⚠️  **LGPL notice:** the underlying `libheif` C library is
> [LGPL-licensed](https://github.com/strukturag/libheif/blob/main/COPYING).
> Linking it makes your final binary LGPL-encumbered.  Review your
> distribution obligations before enabling this feature in a commercial
> product.  See [NOTICE](NOTICE) for full attribution details.

```toml
# Enable experimental HEIC/HEIF support (requires libheif C library):
[dependencies]
img4avif = { version = "0.5", features = ["heic-experimental"] }

# Enable experimental RAW support (pure Rust, no C):
[dependencies]
img4avif = { version = "0.5", features = ["raw-experimental"] }
```

---

## Performance benchmarks

Measurements on an `m6i.large` EC2 (2 vCPU, 8 GB, Amazon Linux 2023,
`RUSTFLAGS="-C target-cpu=native"`).

### Throughput Estimates (quality=80)

| Input size | Encode time | AVIF size | Peak RSS |
|-----------|-------------|-----------|----------|
| 1 MP (1000 × 1000 PNG, speed=6) | ~220 ms | ~45 KB | ~18 MB |
| 10 MP (3162 × 3162 PNG, speed=6) | ~1.8 s | ~350 KB | ~65 MB |
| 50 MP (8944 × 5615 PNG, speed=6) | ~9 s | ~1.6 MB | ~140 MB |
| 100 MB (5000 × 5000 PNG, speed=10) | ~12 s | ~2.2 MB | ~195 MB |
| 200 MP (16383 × 12207 PNG, speed=10) | ~60 s | ~8.5 MB | ~870 MB |

### Lambda cold-start

| Metric | Value |
|--------|-------|
| `Converter::new()` init time | < 1 ms |
| First `convert()` (64 × 64 PNG) | < 50 ms |

> Use speed=10 on Lambda to reduce CPU time at the cost of ~10–15% larger
> files.  The `Config::lambda_cost_optimized()` preset applies this
> automatically.

---

## AWS Lambda Deployment

### 1. Build for Lambda (x86_64)

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

For aarch64 (Graviton2, typically cheaper):

```bash
cargo build --release --target aarch64-unknown-linux-musl
```

### 2. Lambda Layer Configuration

```yaml
# template.yaml (AWS SAM)
Layers:
  - !Sub arn:aws:lambda:${AWS::Region}:${AWS::AccountId}:layer:img4avif:1

Environment:
  Variables:
    # Optional: override quality at runtime
    IMAGINE_AVIF_QUALITY: "80"
```

### 3. Memory Estimates

| Image size | Minimum Lambda memory |
|-----------|--------------------------|
| ≤ 8 MP | 256 MB |
| ≤ 50 MP | 512 MB |
| ≤ 100 MP | 768 MB |
| ≤ 200 MP | 1024 MB+ |

---

## License

- Licensed under the [Apache License, Version 2.0](LICENSE).
- No GPL transitive dependencies in the default build (see LGPL note for `heic-experimental`)

This product includes third-party components whose notices are listed in
[NOTICE](NOTICE).  The most notable is `ravif` (BSD-3-Clause), which provides
the AV1 encoder backend.
