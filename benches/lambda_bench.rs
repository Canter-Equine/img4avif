use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use img4avif::{Config, Converter};

fn make_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([100u8, 150, 200, 255]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

/// Build a 16-bit RGBA PNG.
///
/// 16-bit PNGs exercise the `encode_raw_planes_10_bit` path including the
/// per-pixel BT.601 YCbCr conversion loop in `rgba16_to_10bit_ycbcr_bt601`.
fn make_png_16bit(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgba};
    // Solid mid-grey in 16-bit: each channel = 32768 (50% of 65535).
    let img: ImageBuffer<Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(width, height, Rgba([32768u16, 32768, 32768, 65535]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

fn bench_conversion(c: &mut Criterion) {
    let sizes: &[(u32, u32, &str)] = &[
        (100, 100, "0.01MP"),
        (1000, 1000, "1MP"),
        (3162, 3162, "10MP"),
    ];

    let mut group = c.benchmark_group("img4avif/conversion");
    group.sample_size(10);

    for &(w, h, label) in sizes {
        let png = make_png(w, h);
        let config = Config::default().quality(80).speed(10);

        group.bench_with_input(BenchmarkId::new("png_to_avif", label), &png, |b, input| {
            b.iter(|| {
                let converter = Converter::new(config.clone()).unwrap();
                converter.convert(input).unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark the 16-bit PNG → 10-bit AVIF path.
///
/// This path calls `ravif::Encoder::encode_raw_planes_10_bit` and runs the
/// per-pixel `rgba16_to_10bit_ycbcr_bt601` conversion loop, making it a
/// distinct workload from the 8-bit path.
fn bench_conversion_16bit(c: &mut Criterion) {
    let sizes: &[(u32, u32, &str)] = &[(100, 100, "0.01MP"), (1000, 1000, "1MP")];

    let mut group = c.benchmark_group("img4avif/conversion_16bit");
    group.sample_size(10);

    for &(w, h, label) in sizes {
        let png16 = make_png_16bit(w, h);
        let config = Config::default().quality(80).speed(10);

        group.bench_with_input(
            BenchmarkId::new("png16_to_avif", label),
            &png16,
            |b, input| {
                b.iter(|| {
                    let converter = Converter::new(config.clone()).unwrap();
                    converter.convert(input).unwrap()
                });
            },
        );
    }

    group.finish();
}

fn bench_cold_start(c: &mut Criterion) {
    let png = make_png(100, 100);

    c.bench_function("cold_start_converter_init", |b| {
        b.iter(|| {
            let _converter = Converter::new(Config::default()).unwrap();
        });
    });

    c.bench_function("cold_start_full_convert_100px", |b| {
        b.iter(|| {
            let converter = Converter::new(Config::default()).unwrap();
            converter.convert(&png).unwrap()
        });
    });
}

criterion_group!(
    benches,
    bench_conversion,
    bench_conversion_16bit,
    bench_cold_start
);
criterion_main!(benches);
