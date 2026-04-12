# Changelog

All notable changes to `img4avif` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
I forgot to CHANGELOG the updates from 0.2-0.4, sorry.

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
