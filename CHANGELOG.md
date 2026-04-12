# Changelog

All notable changes to `img2avif` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.5.0] — 2026-04-12

### Added

- **200 MP stress test** (`two_hundred_megapixel_converts_successfully`): new
  ignored test that converts a ~200 MP synthetic PNG with a 1 GiB memory limit
  and verifies end-to-end success.
- **200 MP limit guard test** (`two_hundred_megapixel_exceeds_default_512mib_limit`):
  confirms that `Error::MemoryExceeded` is raised on Linux when a 200 MP image
  is submitted with the default 512 MiB budget.
- **100 MB image stress test** (`hundred_mb_image_converts_successfully`): new
  ignored test that converts a 5000 × 5000 synthetic PNG (~100 MB pixel data)
  and verifies peak RSS stays under 512 MiB — proving 512 MB Lambda viability
  for this workload class.
- **Binary size CI job** now runs on every push and pull-request (previously
  only ran on schedule / workflow_dispatch).
- **Memory stress CI job** now runs on every push and pull-request; it runs the
  100 MB image test and the 50 MP test (previously only ran on schedule /
  workflow_dispatch and only the 50 MP test).
- **CI artifact upload**: `examples/out/` AVIF outputs from the integration
  fixture tests on Ubuntu are uploaded as the `examples-out` artifact so
  outputs are browsable in the GitHub Actions UI.

### Changed

- **Version bumped to 0.5.0**.
- **`Cargo.toml` `exclude`**: `examples/fixtures/` is now excluded from the
  published crate so developers who add `img2avif` as a dependency do not
  receive the example/test image files.
- **Memory Guard documentation** in `README.md` updated to include a table
  mapping image sizes to minimum recommended `memory_limit_bytes` values (up
  to 200 MP / 1024 MiB), replacing the previous single-sentence description
  that only mentioned 50 MP.
- **Performance benchmarks** in `README.md` now include rows for a 100 MB
  (5000 × 5000) and 200 MP (16383 × 12207) workload with `speed=10`.
- **Lambda memory/cost table** in `README.md` updated: 50 MP now correctly
  shows 512 MB (the default limit covers it), added 100 MB row, and removed
  the previously incorrect 768 MB recommendation for ≤ 50 MP images.
- **Installation snippet** in `README.md` updated to reference `"0.5"`.

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
