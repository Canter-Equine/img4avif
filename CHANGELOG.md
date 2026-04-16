# Changelog

Notable changes to `img4avif` are documented here, trying to keep up with I but forgot to CHANGELOG the updates from 0.2-0.4, sorry.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.6.0] — 2026-04-16

**BREAKING CHANGES**

- **Quality scale normalization**: `Config::quality` and `Config::alpha_quality` now use a **1–10** scale (matching `Config::speed`) instead of the previous 1–100 scale, and encoder behavior is preserved.
  - **Default values**: `quality` and `alpha_quality` defaults to `8`
  - **Lambda preset**: `lambda_cost_optimized()` defailts to `8`
  - **Migration**: Divide your existing quality values by 10 (e.g., `quality(80)` → `quality(8)`)

### Added

- **Parallel processing with rayon**: `convert_multi` now encodes multiple resolutions in parallel on native targets for speed improvement on compatible chipsets.
- **New `convert_batch` method**: Process multiple independent images in parallel. Each image is decoded and encoded on a separate thread, providing coarse-grained parallelism for batch workloads.
- **Alpha quality optimization**: The encoder now detects transparency in images. When an image is fully opaque (no alpha channel variation), `alpha_quality` is automatically treated as a no-op to save processing resources.

---

## [0.5.2] — 2026-04-12

### Changed

- Version bump to 0.5.2.

---

## [0.5.0] — 2026-04-12

### Added

- **Stress Test**: We stress test the crate on 200 MP image, on a 100 MB image, and a handful of file formats.
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
