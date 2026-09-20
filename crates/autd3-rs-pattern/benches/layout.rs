use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use autd3_rs_core::Length;
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_pattern::focus_transducer;

const N: usize = Autd3::NUM_TRANSDUCERS;
const HEADER: usize = 8;
const DEV_STRIDE: usize = HEADER + 2 * N;

const DEVICE_COUNTS: &[usize] = &[1, 9, 64, 256];

fn make_geometry(devices: usize) -> Geometry {
    let devs: Vec<Autd3> = (0..devices)
        .map(|i| {
            Autd3::new(
                Point3::new(i as f32 * 200.0, 0.0, 0.0),
                UnitQuaternion::identity(),
            )
        })
        .collect();
    Geometry::new(devs)
}

fn compute_array(geo: &Geometry, target: Point3<f32>, wl: Length, buf: &mut [[Phase; N]]) {
    for (slot, dev) in buf.iter_mut().zip(geo.iter()) {
        for (p, &pos) in slot.iter_mut().zip(dev.positions()) {
            *p = focus_transducer(pos, target, wl);
        }
    }
}

fn compute_vec(geo: &Geometry, target: Point3<f32>, wl: Length, buf: &mut [Vec<Phase>]) {
    for (slot, dev) in buf.iter_mut().zip(geo.iter()) {
        for (p, &pos) in slot.iter_mut().zip(dev.positions()) {
            *p = focus_transducer(pos, target, wl);
        }
    }
}

fn pack(phases: &[Phase], intensities: &[Intensity], dst: &mut [u8]) {
    for (i, (p, a)) in phases.iter().zip(intensities).enumerate() {
        dst[i] = p.0;
        dst[N + i] = a.0;
    }
}

fn pack_array(phases: &[[Phase; N]], intensities: &[[Intensity; N]], dst: &mut [u8]) {
    for (d, (p, a)) in phases.iter().zip(intensities).enumerate() {
        let base = d * DEV_STRIDE + HEADER;
        pack(p, a, &mut dst[base..base + 2 * N]);
    }
}

fn pack_vec(phases: &[Vec<Phase>], intensities: &[Vec<Intensity>], dst: &mut [u8]) {
    for (d, (p, a)) in phases.iter().zip(intensities).enumerate() {
        let base = d * DEV_STRIDE + HEADER;
        pack(p, a, &mut dst[base..base + 2 * N]);
    }
}

fn bench(c: &mut Criterion) {
    let wl = Length::from_mm(8.5);
    let target = Point3::new(90.0, 70.0, 150.0);

    let mut g_compute = c.benchmark_group("compute");
    for &devices in DEVICE_COUNTS {
        let geo = make_geometry(devices);
        let mut arr = vec![[Phase::ZERO; N]; devices];
        let mut vc = vec![vec![Phase::ZERO; N]; devices];

        g_compute.bench_with_input(BenchmarkId::new("array", devices), &devices, |b, _| {
            b.iter(|| {
                compute_array(&geo, target, wl, &mut arr);
                black_box(&arr);
            });
        });
        g_compute.bench_with_input(BenchmarkId::new("vec", devices), &devices, |b, _| {
            b.iter(|| {
                compute_vec(&geo, target, wl, &mut vc);
                black_box(&vc);
            });
        });
    }
    g_compute.finish();

    let mut g_pack = c.benchmark_group("compute_plus_pack");
    for &devices in DEVICE_COUNTS {
        let geo = make_geometry(devices);
        let mut arr = vec![[Phase::ZERO; N]; devices];
        let mut vc = vec![vec![Phase::ZERO; N]; devices];
        let arr_intensities = vec![[Intensity::MAX; N]; devices];
        let vc_intensities = vec![vec![Intensity::MAX; N]; devices];
        let mut dst = vec![0u8; devices * DEV_STRIDE];

        g_pack.bench_with_input(BenchmarkId::new("array", devices), &devices, |b, _| {
            b.iter(|| {
                compute_array(&geo, target, wl, &mut arr);
                pack_array(&arr, &arr_intensities, &mut dst);
                black_box(&dst);
            });
        });
        g_pack.bench_with_input(BenchmarkId::new("vec", devices), &devices, |b, _| {
            b.iter(|| {
                compute_vec(&geo, target, wl, &mut vc);
                pack_vec(&vc, &vc_intensities, &mut dst);
                black_box(&dst);
            });
        });
    }
    g_pack.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
