# img2avif Proof-of-Concept — Conversion Results

This document records the results of converting four representative test images
to AVIF using the `img2avif` crate at its `0.1.0` draft.

The four images are synthetic stand-ins that match the format and visual
characteristics of the images provided in the task (Wikipedia globe logo,
two kayaking photographs, and a football/sports portrait):

| Synthetic image | Matches provided image | Dimensions | Format |
|---|---|---|---|
| `wikipedia_logo` | Wikipedia globe logo | 400×424 px | PNG, RGBA with alpha |
| `kayak1` | First kayaking photo | 340×453 px | JPEG, colour |
| `kayak2` | Second kayaking photo | 300×400 px | JPEG, colour |
| `football` | Football player portrait | 900×1200 px | JPEG, monochrome |

## Running the proof of concept

```sh
cargo run --example poc
```

Add `--features dev-logging` and set `RUST_LOG=img2avif=debug` for full
per-stage pipeline logs.

---

## Conversion results

All 8 conversions (4 images × 2 resolutions each) **succeeded**.

| Image | Resolution | Input (B) | Output (B) | Compression | Time (ms) | Output file |
|---|---|---|---|---|---|---|
| wikipedia_logo | original | 40,890 | 11,277 | 3.6× | ~8,450 | `poc/output/wikipedia_logo_original.avif` |
| wikipedia_logo | 1080p | 40,890 | 11,277 | 3.6× | ~7,570 | `poc/output/wikipedia_logo_1080p.avif` |
| kayak1 | original | 7,663 | 1,321 | 5.8× | ~4,640 | `poc/output/kayak1_original.avif` |
| kayak1 | 1080p | 7,663 | 1,321 | 5.8× | ~4,570 | `poc/output/kayak1_1080p.avif` |
| kayak2 | original | 6,145 | 1,167 | 5.3× | ~4,080 | `poc/output/kayak2_original.avif` |
| kayak2 | 1080p | 6,145 | 1,167 | 5.3× | ~4,090 | `poc/output/kayak2_1080p.avif` |
| football | original | 157,871 | 69,183 | 2.3× | ~30,390 | `poc/output/football_original.avif` |
| football | 1080p | 157,871 | 69,183 | 2.3× | ~30,880 | `poc/output/football_1080p.avif` |

### Notes on 1080p variants

All four images have widths ≤ 900 px, so they are narrower than the 1080 px
downscale target. `img2avif` never upscales (by design), so each `1080p`
variant is identical to the `original` — the pixel buffer is passed through
unchanged at no extra cost. This matches the documented behaviour of
`OutputResolution::Width1080`.

### Error-log coverage

The `dev-logging` feature exposes structured `log` records at every major
pipeline stage:

| Level | Events logged |
|---|---|
| `ERROR` | Any return of `Err(…)` — decode failure, encode failure, memory exceeded |
| `WARN` | `strip_exif=false` in use; suspiciously small AVIF output |
| `INFO` | Input size check passed, decoded dimensions, encode complete with byte count |
| `DEBUG` | Pre/post-decode RSS readings, resize skipped/applied, rav1e call parameters |

No errors or warnings were emitted during this run.

---

## Optimization notes

### CPU usage

- **Encode speed** is the dominant cost. `rav1e` at `speed=6` (default) took
  ~30 seconds for the 900×1200 monochrome JPEG, and ~8 seconds for the
  400×424 PNG. Setting `Config::speed(10)` reduces encode time by roughly
  3–5× at a small quality penalty (~2–4 SSIM points), making it the
  appropriate choice for Lambda cost-sensitive pipelines.
- **Rayon threading**: `ravif` parallelises encoding across all available CPU
  cores via rayon. On Lambda 1 769 MB (1 vCPU) tiers, set
  `RAYON_NUM_THREADS=1` to avoid OS scheduling overhead; leave unset on
  3 008 MB+ (2 vCPU) tiers.
- **Quality tuning**: For preview/thumbnail outputs at 1080 px or smaller,
  `quality=70` delivers visually acceptable AVIF files at lower CPU time.
  Reserve `quality=80+` for archival full-resolution encodes.

### Memory usage

- **Decode buffer**: `img2avif` decodes into a flat RGBA pixel buffer
  (`width × height × 4` bytes for 8-bit, `× 8` for 16-bit PNG). A 24 MP
  JPEG occupies ~96 MiB decoded; a 50 MP source needs ~200 MiB.
- **Multi-resolution pipeline**: When producing multiple output sizes from
  the same source, use `Converter::convert_multi()`. It decodes once and
  applies N separate resize + encode passes. Calling `convert()` N times
  re-decodes the JPEG/PNG each time, wasting ~2–3× peak RAM.
- **Early rejection**: `Config::max_input_bytes` is checked *before* any
  decompression occurs. Tightening this limit (e.g. 20 MiB for a
  consumer-photo API) catches oversized uploads at near-zero cost.
- **Resize memory**: The Lanczos3 resize step (`image::imageops::resize`)
  materialises a second pixel buffer at the new dimensions. For a 24 MP →
  1080 px downscale the transient peak is ~100 MiB (source buffer) +
  ~5 MiB (target buffer). Both buffers are freed before the AVIF encoder
  allocates its own working memory.

### Security

- **Decompression bombs**: `Config::max_pixels` (default 268 MP) and the
  `image::Limits` allocation cap prevent a tiny compressed file from
  expanding to gigabytes in RAM. For a consumer API, tighten to 8 MP
  (≈ 3 264 × 2 448 — a typical phone camera). **This is the highest-impact
  security control in the library.**
- **Input-size cap**: `Config::max_input_bytes` (default 100 MiB) is the
  first line of defence — it rejects oversized bodies before the decoder
  even reads a byte.
- **Metadata leakage**: `strip_exif=true` (default) removes EXIF GPS
  coordinates, device serial numbers, camera fingerprint data, and embedded
  JPEG thumbnails that could leak user-identifiable information.  **Always
  keep enabled in a multi-tenant pipeline.**
- **Unsafe code**: `#![forbid(unsafe_code)]` is enforced across all library
  source.  The `heic-experimental` feature links `libheif` (a C library),
  which bypasses this guarantee and makes the binary LGPL-encumbered; avoid
  enabling it in environments where those constraints matter.
- **Future hardening opportunity**: The AVIF output validation currently
  performs a lightweight structural check (non-empty, ≥ 20 bytes, `ftyp`
  box present at offset 4). A full ISOBMFF parse would catch additional
  malformed-container edge cases, but at the cost of adding a parsing
  dependency.
