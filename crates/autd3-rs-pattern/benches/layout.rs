// Rationale for the per-device pattern buffer layout (kept as a regression guard and as
// the evidence behind the design choice). Compares, under a PRE-ALLOCATED & REUSED buffer
// (allocation excluded, since `Geometry::pattern_buffer()` is built once and reused):
//
//   ARRAY : Vec<[Emission; 249]>   (former layout — contiguous, compile-time length)
//   VEC   : Vec<Vec<Emission>>     (current layout — per-device heap block)
//
// Both variants run the IDENTICAL math (`focus_transducer`) and the IDENTICAL wire-pack
// loop, differing only in the container type, so the benchmark isolates the layout effect.
// Measured difference is within noise (±3%), which is why the codebase standardised on the
// ergonomic `Vec<Vec<Emission>>` everywhere.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use autd3_rs_core::Length;
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs_core::value::Emission;
use autd3_rs_pattern::{FocusOption, focus_transducer};

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

fn compute_array(
    geo: &Geometry,
    target: Point3<f32>,
    wl: Length,
    opt: FocusOption,
    buf: &mut [[Emission; N]],
) {
    for (slot, dev) in buf.iter_mut().zip(geo.iter()) {
        for (e, &pos) in slot.iter_mut().zip(dev.positions()) {
            *e = focus_transducer(pos, target, wl, &opt);
        }
    }
}

fn compute_vec(
    geo: &Geometry,
    target: Point3<f32>,
    wl: Length,
    opt: FocusOption,
    buf: &mut [Vec<Emission>],
) {
    for (slot, dev) in buf.iter_mut().zip(geo.iter()) {
        for (e, &pos) in slot.iter_mut().zip(dev.positions()) {
            *e = focus_transducer(pos, target, wl, &opt);
        }
    }
}

fn pack_array(buf: &[[Emission; N]], out: &mut [u8]) {
    for (d, slot) in buf.iter().enumerate() {
        let base = d * DEV_STRIDE + HEADER;
        for (i, e) in slot.iter().enumerate() {
            out[base + 2 * i] = e.phase.0;
            out[base + 2 * i + 1] = e.intensity.0;
        }
    }
}

fn pack_vec(buf: &[Vec<Emission>], out: &mut [u8]) {
    for (d, slot) in buf.iter().enumerate() {
        let base = d * DEV_STRIDE + HEADER;
        for (i, e) in slot.iter().enumerate() {
            out[base + 2 * i] = e.phase.0;
            out[base + 2 * i + 1] = e.intensity.0;
        }
    }
}

fn bench(c: &mut Criterion) {
    let wl = Length::millimeters(8.5);
    let opt = FocusOption::default();
    let target = Point3::new(90.0, 70.0, 150.0);

    let mut g_compute = c.benchmark_group("compute");
    for &devices in DEVICE_COUNTS {
        let geo = make_geometry(devices);
        let mut arr = vec![[Emission::default(); N]; devices];
        let mut vc = vec![vec![Emission::default(); N]; devices];

        g_compute.bench_with_input(BenchmarkId::new("array", devices), &devices, |b, _| {
            b.iter(|| {
                compute_array(&geo, target, wl, opt, &mut arr);
                black_box(&arr);
            });
        });
        g_compute.bench_with_input(BenchmarkId::new("vec", devices), &devices, |b, _| {
            b.iter(|| {
                compute_vec(&geo, target, wl, opt, &mut vc);
                black_box(&vc);
            });
        });
    }
    g_compute.finish();

    let mut g_pack = c.benchmark_group("compute_plus_pack");
    for &devices in DEVICE_COUNTS {
        let geo = make_geometry(devices);
        let mut arr = vec![[Emission::default(); N]; devices];
        let mut vc = vec![vec![Emission::default(); N]; devices];
        let mut out = vec![0u8; devices * DEV_STRIDE];

        g_pack.bench_with_input(BenchmarkId::new("array", devices), &devices, |b, _| {
            b.iter(|| {
                compute_array(&geo, target, wl, opt, &mut arr);
                pack_array(&arr, &mut out);
                black_box(&out);
            });
        });
        g_pack.bench_with_input(BenchmarkId::new("vec", devices), &devices, |b, _| {
            b.iter(|| {
                compute_vec(&geo, target, wl, opt, &mut vc);
                pack_vec(&vc, &mut out);
                black_box(&out);
            });
        });
    }
    g_pack.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
