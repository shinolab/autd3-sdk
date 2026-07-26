use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use autd3_rs_core::Length;
use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::Emission;
use autd3_rs_pattern_holo::{
    ControlPoint, GreedyOption, GsOption, GspatOption, NaiveOption, Pa, greedy, gs, gspat, naive,
};

const DEVICE_COUNTS: &[usize] = &[1, 9, 64];
const FOCI_COUNTS: &[usize] = &[1, 4, 16];

fn make_geometry(devices: usize) -> Geometry {
    Geometry::new(
        (0..devices)
            .map(|i| {
                Autd3::new(
                    Point3::new(i as f32 * 200.0, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    )
}

fn make_foci(geometry: &Geometry, n: usize) -> Vec<ControlPoint> {
    (0..n)
        .map(|i| ControlPoint {
            point: geometry.center() + Vector3::new(i as f32 * 10.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        })
        .collect()
}

fn make_buffer(geometry: &Geometry) -> Vec<Vec<Emission>> {
    vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()]
}

fn wavelength() -> Length {
    autd3_rs_pattern::wavelength(340.0 * m / s)
}

fn bench(c: &mut Criterion) {
    let wl = wavelength();
    let mut group = c.benchmark_group("holo");
    for &d in DEVICE_COUNTS {
        let geometry = make_geometry(d);
        let mut dst = make_buffer(&geometry);
        for &n in FOCI_COUNTS {
            let foci = make_foci(&geometry, n);
            let id = format!("{d}dev-{n}foci");

            group.bench_with_input(BenchmarkId::new("naive", &id), &foci, |b, foci| {
                b.iter(|| {
                    naive(
                        &geometry,
                        foci,
                        wl,
                        &NaiveOption::default(),
                        black_box(&mut dst),
                    )
                    .unwrap();
                });
            });
            group.bench_with_input(BenchmarkId::new("gs", &id), &foci, |b, foci| {
                b.iter(|| {
                    gs(
                        &geometry,
                        foci,
                        wl,
                        &GsOption::default(),
                        black_box(&mut dst),
                    )
                    .unwrap();
                });
            });
            group.bench_with_input(BenchmarkId::new("gspat", &id), &foci, |b, foci| {
                b.iter(|| {
                    gspat(
                        &geometry,
                        foci,
                        wl,
                        &GspatOption::default(),
                        black_box(&mut dst),
                    )
                    .unwrap();
                });
            });
            group.bench_with_input(BenchmarkId::new("greedy", &id), &foci, |b, foci| {
                b.iter(|| {
                    greedy(
                        &geometry,
                        foci,
                        wl,
                        &GreedyOption::default(),
                        black_box(&mut dst),
                    )
                    .unwrap();
                });
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
