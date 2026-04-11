use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use img2avif::{Config, Converter};

fn make_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([100u8, 150, 200, 255]));
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

    let mut group = c.benchmark_group("img2avif/conversion");
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

criterion_group!(benches, bench_conversion, bench_cold_start);
criterion_main!(benches);
