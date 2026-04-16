# Changelog

All notable changes to `img4avif` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
I forgot to CHANGELOG the updates from 0.2-0.4, sorry.

---

## [0.6.0] — 2026-04-16

### Added

- **Parallel processing with rayon**: `convert_multi` now encodes multiple resolutions in parallel on native targets (non-WASM), leveraging all available CPU cores for significant performance improvements.
- **New `convert_batch` method**: Process multiple independent images in parallel. Each image is decoded and encoded on a separate thread, providing coarse-grained parallelism for batch workloads.
- **Alpha quality optimization**: The encoder now detects transparency in images. When an image is fully opaque (no alpha channel variation), `alpha_quality` is automatically treated as a no-op to save processing resources.
- **Transparency detection**: New `RawImage::has_transparency()` method scans the alpha channel to determine if any pixels have transparency.

### Changed — ⚠️ **BREAKING CHANGES**

- **Quality scale normalization**: `Config::quality` and `Config::alpha_quality` now use a **1–10** scale (matching `Config::speed`) instead of the previous 1–100 scale.
  - **Default values**: `quality` and `alpha_quality` changed from `80` → `8`
  - **Lambda preset**: `lambda_cost_optimized()` preset changed from `75` → `8`
  - **Migration**: Divide your existing quality values by 10 (e.g., `quality(80)` → `quality(8)`)
  - **Internal mapping**: The 1–10 user-facing values are scaled back to 1–100 when calling `ravif`, so encoder behavior is preserved

### Migration Guide (0.5.x → 0.6.0)

```rust
// Before (0.5.x)
Config::default()
    .quality(80)
    .alpha_quality(95)

// After (0.6.0)
Config::default()
    .quality(8)
    .alpha_quality(10)  // rounded from 95 → 10
```

---

## [0.5.2] — 2026-04-12

### Changed

- Version bump to 0.5.2.

---

## [0.5.0] — 2026-04-12

### Added

- **200 MP stress test** (`two_hundred_megapixel_converts_successfully`): converts
  a ~200 MP synthetic PNG with a 1 GiB memory limit and verifies end-to-end success.
- **200 MP limit guard test** (`two_hundred_megapixel_exceeds_default_512mib_limit`):
  confirms that `Error::MemoryExceeded` is raised on Linux when a 200 MP image
  is submitted with the default 512 MiB budget.
- **100 MB image stress test** (`hundred_mb_image_converts_successfully`): converts a
  5000 × 5000 synthetic PNG (~100 MB pixel data) and verifies peak RSS stays under
  512 MiB — proving 512 MB Lambda viability for this workload class.
- **CI artifact upload**: `examples/out/` AVIF outputs from the CI pipeline tests are
  uploaded and manually reviewed for quality assurance.

### Fixed

- The `memory-stress` and `binary-size` CI jobs were previously gated behind
  `schedule` / `workflow_dispatch` events only, meaning they never ran on
  normal pull-requests. They now run unconditionally.

---

## [0.1.0] — initial release

- Core JPEG / PNG / WebP → AVIF conversion with pure-Rust `rav1e` encoder.
- `MemoryGuard` with configurable RSS limit (default 512 MiB).
- Multi-resolution output (`convert_multi`).
- EXIF strip-by-default with opt-in preservation.
- HDR10 16-bit PNG input support.
- `Config::lambda_cost_optimized()` preset.
- `dev-logging` feature flag for structured pipeline log output.
- Experimental `heic-experimental` and `raw-experimental` feature flags.
