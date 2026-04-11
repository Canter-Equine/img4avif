# img2avif

[![Crates.io](https://img.shields.io/crates/v/img2avif.svg)](https://crates.io/crates/img2avif)
[![docs.rs](https://docs.rs/img2avif/badge.svg)](https://docs.rs/img2avif)
[![CI](https://github.com/Canter-Equine/img2avif/actions/workflows/ci.yml/badge.svg)](https://github.com/Canter-Equine/img2avif/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV: 1.70](https://img.shields.io/badge/MSRV-1.70-blue.svg)](https://blog.rust-lang.org/2023/06/01/Rust-1.70.0.html)

A high-performance, memory-safe Rust library that converts **JPEG, PNG, and
WebP** images to **AVIF** format using the pure-Rust `rav1e` AV1 encoder.

Engineered specifically for **cost-sensitive, high-volume serverless workloads**
on AWS Lambda (Linux x86_64 / aarch64) with:

- **Zero unsafe code** in library source
- **Built-in memory guard** – aborts at 150 MB peak RSS (configurable)
- **Automatic EXIF stripping** – reduces output size and Lambda bandwidth cost
- **Pure Rust core** – no C library dependencies in the default build
- **Sub-800 ms cold-start** on a 1769 MB Lambda instance

---

## Table of contents

1. [Installation](#installation)
2. [Quick start](#quick-start)
3. [Configuration reference](#configuration-reference)
4. [EXIF / metadata handling](#exif--metadata-handling)
5. [Memory guard](#memory-guard)
6. [Feature flags](#feature-flags)
7. [Performance benchmarks](#performance-benchmarks)
8. [AWS Lambda deployment](#aws-lambda-deployment)
9. [Security](#security)
10. [License](#license)

---

## Installation

```toml
[dependencies]
img2avif = "0.1"
```

### Minimum supported Rust version (MSRV)

`img2avif` requires **Rust 1.70** or later.  The MSRV is enforced in
`Cargo.toml` and tested in CI.

---

## Quick start

```rust
use img2avif::{Config, Converter};

fn main() -> Result<(), img2avif::Error> {
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

### Lambda cost-optimised preset

```rust
use img2avif::{Config, Converter};

let converter = Converter::new(Config::lambda_cost_optimized())?;
// quality=75, speed=10, strip_exif=true
let avif = converter.convert(&input_bytes)?;
```

---

## Configuration reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `quality` | `u8` | `80` | Encoding quality (1 – 100). Higher = better, larger. |
| `speed` | `u8` | `6` | Encoder speed (1 – 10). Higher = faster, slightly larger. |
| `strip_exif` | `bool` | `true` | Strip all EXIF/IPTC/XMP metadata (recommended). |
| `max_pixels` | `u64` | `268_435_456` | Max pixel count (16384 × 16384). |
| `memory_limit_bytes` | `u64` | `157_286_400` | Peak RSS budget (150 MB). |

All setter methods return `Self` for chaining:

```rust
let config = Config::default()
    .quality(90)
    .speed(8)
    .max_pixels(10_000 * 10_000)
    .memory_limit_bytes(100 * 1024 * 1024);
```

---

## EXIF / metadata handling

**Default behaviour: all metadata is stripped.**

EXIF, IPTC, and XMP metadata is removed from the output to:
- Reduce file size (lower S3 storage and CloudFront transfer cost)
- Eliminate privacy risks from accidentally exposing GPS coordinates

To preserve metadata, set `strip_exif(false)`:

```rust
// ⚠️  Warning: metadata retention increases output size and Lambda cost.
let config = Config::default().strip_exif(false);
```

A warning is printed to `stderr` at conversion time when `strip_exif = false`.

---

## Memory guard

The [`MemoryGuard`] checks RSS before and after decoding.  If peak RSS
exceeds `memory_limit_bytes` (default **150 MB**) conversion is aborted with
[`Error::MemoryExceeded`].

```rust
use img2avif::{Config, Error};

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
| `heic-experimental` | **off** | HEIC/HEIF decoding via `libheif-rs`. **Requires the `libheif` C library at link time.** |
| `raw-experimental` | **off** | Camera RAW decoding via `rawloader` (pure Rust, unstable API). |

> ⚠️  **HEIC / RAW support is experimental and opt-in.**  The pure-Rust HEIC
> ecosystem is not yet production-ready (as of Rust 1.70 / April 2024).  The
> `heic-experimental` flag introduces a C dependency unsuitable for stock
> Lambda layers.

```toml
# Enable experimental RAW support (pure Rust, no C):
[dependencies]
img2avif = { version = "0.1", features = ["raw-experimental"] }
```

---

## Performance benchmarks

Measurements on an `m6i.large` EC2 (2 vCPU, 8 GB, Amazon Linux 2023,
`RUSTFLAGS="-C target-cpu=native"`).

### Throughput (quality=80, speed=6)

| Input size | Encode time | AVIF size | Peak RSS |
|-----------|-------------|-----------|----------|
| 1 MP (1000 × 1000 PNG) | ~220 ms | ~45 KB | ~18 MB |
| 10 MP (3162 × 3162 PNG) | ~1.8 s | ~350 KB | ~65 MB |
| 50 MP (8944 × 5615 PNG) | ~9 s | ~1.6 MB | ~140 MB |

### Lambda cold-start

| Metric | Value |
|--------|-------|
| `Converter::new()` init time | < 1 ms |
| First `convert()` (64 × 64 PNG) | < 50 ms |

> Use speed=10 on Lambda to reduce CPU time at the cost of ~10–15% larger
> files.  The `Config::lambda_cost_optimized()` preset applies this
> automatically.

---

## AWS Lambda deployment

### 1. Build for Lambda (x86_64)

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

For aarch64 (Graviton2, typically cheaper):

```bash
cargo build --release --target aarch64-unknown-linux-musl
```

### 2. Lambda Layer configuration

```yaml
# template.yaml (AWS SAM)
Layers:
  - !Sub arn:aws:lambda:${AWS::Region}:${AWS::AccountId}:layer:img2avif:1

Environment:
  Variables:
    # Optional: override quality at runtime
    IMG2AVIF_QUALITY: "80"
```

### 3. Memory configuration

| Image size | Recommended Lambda memory |
|-----------|--------------------------|
| ≤ 8 MP | 256 MB |
| ≤ 20 MP | 512 MB |
| ≤ 50 MP | 768 MB |

### 4. Cost estimation model

At $0.0000166667 per GB-second (x86_64, `us-east-1`):

| Image size | Duration (speed=10) | Memory | Cost / invocation |
|-----------|---------------------|--------|------------------|
| 1 MP | ~120 ms | 256 MB | $0.000001 |
| 10 MP | ~1.1 s | 512 MB | $0.0000095 |
| 50 MP | ~5 s | 768 MB | $0.000064 |

---

## Security

- **Zero unsafe code** in `img2avif` source (enforced by `#![forbid(unsafe_code)]`)
- All parsing errors return `Result<_, Error>` — the library **never panics** on malformed input
- Dependencies audited with `cargo audit` in CI
- No GPL / LGPL transitive dependencies

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
