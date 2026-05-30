use criterion::{black_box, criterion_group, criterion_main, Criterion};

use fitsview_core::{
    colormap::{render_to_rgba, Colormap},
    contour::{compute_contours, ContourLevel},
    scale::{compute_scale, ScaleMode},
    stats::compute_stats,
    wcs::Wcs,
};

fn make_image(w: usize, h: usize) -> Vec<f32> {
    (0..w * h).map(|i| (i as f32).sin() * 1000.0 + 5000.0).collect()
}

fn bench_zscale(c: &mut Criterion) {
    let data = make_image(1024, 1024);
    c.bench_function("zscale 1024x1024", |b| {
        b.iter(|| compute_scale(black_box(&data), ScaleMode::ZScale))
    });
}

fn bench_render_rgba(c: &mut Criterion) {
    let data = make_image(1024, 1024);
    let sr = compute_scale(&data, ScaleMode::ZScale);
    c.bench_function("render_to_rgba 1024x1024", |b| {
        b.iter(|| {
            render_to_rgba(
                black_box(&data),
                sr.vmin, sr.vmax,
                Colormap::Viridis,
                ScaleMode::ZScale,
                1.0, 0.5,
                None,
            )
        })
    });
}

fn bench_stats(c: &mut Criterion) {
    let data = make_image(1024, 1024);
    c.bench_function("compute_stats 1024x1024", |b| {
        b.iter(|| compute_stats(black_box(&data)))
    });
}

fn bench_contours(c: &mut Criterion) {
    let data = make_image(512, 512);
    let levels = vec![
        ContourLevel { value: 4000.0, color: [255, 255, 0, 255] },
        ContourLevel { value: 5000.0, color: [0, 255, 0, 255] },
        ContourLevel { value: 6000.0, color: [255, 0, 0, 255] },
    ];
    c.bench_function("compute_contours 512x512 3-levels", |b| {
        b.iter(|| compute_contours(black_box(&data), 512, 512, black_box(&levels)))
    });
}

fn bench_wcs_convert(c: &mut Criterion) {
    use std::collections::HashMap;
    let mut header = HashMap::new();
    header.insert("CRPIX1".to_owned(), "512".to_owned());
    header.insert("CRPIX2".to_owned(), "512".to_owned());
    header.insert("CRVAL1".to_owned(), "180.0".to_owned());
    header.insert("CRVAL2".to_owned(), "30.0".to_owned());
    header.insert("CD1_1".to_owned(), "-0.0001".to_owned());
    header.insert("CD1_2".to_owned(), "0.0".to_owned());
    header.insert("CD2_1".to_owned(), "0.0".to_owned());
    header.insert("CD2_2".to_owned(), "0.0001".to_owned());
    header.insert("CTYPE1".to_owned(), "RA---TAN".to_owned());
    header.insert("CTYPE2".to_owned(), "DEC--TAN".to_owned());

    let wcs = Wcs::from_header(&header).expect("wcs");
    c.bench_function("wcs pixel_to_world", |b| {
        b.iter(|| wcs.pixel_to_world(black_box(256.0), black_box(256.0)))
    });
}

criterion_group!(
    benches,
    bench_zscale,
    bench_render_rgba,
    bench_stats,
    bench_contours,
    bench_wcs_convert,
);
criterion_main!(benches);
