use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::Emission;
use autd3_rs_pattern_holo::*;
use autd3_rs_pattern_holo_wgpu::WgpuBackend;
use std::time::Instant;

#[test]
#[ignore = "requires a GPU; run explicitly to re-measure"]
fn throughput_vs_nalgebra() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("no GPU");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);
    println!(
        "{:<8} {:>4} {:>5} {:>12} {:>12} {:>8}",
        "alg", "dev", "foci", "cpu[ms]", "gpu[ms]", "speedup"
    );
    for d in [9usize, 64] {
        let g = Geometry::new(
            (0..d)
                .map(|i| {
                    Autd3::new(
                        Point3::new(i as f32 * 200.0, 0.0, 0.0),
                        UnitQuaternion::identity(),
                    )
                })
                .collect(),
        );
        for nf in [1usize, 16] {
            let f: Vec<ControlPoint> = (0..nf)
                .map(|i| ControlPoint {
                    point: g.center() + Vector3::new(i as f32 * 10.0, 0.0, 150.0),
                    amplitude: 5e3 * Pa,
                })
                .collect();
            let mut dst = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; d];
            for (name, run) in [("naive", 0), ("gs", 1), ("gspat", 2)] {
                let mut cpu_ms = 0.0;
                let mut gpu_ms = 0.0;
                for (tag, acc) in [(0, &mut cpu_ms), (1, &mut gpu_ms)] {
                    for _ in 0..2 {
                        call(tag, run, &gpu, &g, &f, wl, &mut dst);
                    }
                    let t = Instant::now();
                    let iters = 5.0;
                    for _ in 0..5 {
                        call(tag, run, &gpu, &g, &f, wl, &mut dst);
                    }
                    *acc = t.elapsed().as_secs_f64() / iters * 1e3;
                }
                println!(
                    "{name:<8} {d:>4} {nf:>5} {cpu_ms:>12.2} {gpu_ms:>12.2} {:>7.2}x",
                    cpu_ms / gpu_ms
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU; run explicitly to re-measure"]
fn batch_vs_sequential() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("no GPU");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);
    println!(
        "{:<8} {:>4} {:>5} {:>9} {:>14} {:>13} {:>8}",
        "alg", "dev", "foci", "problems", "sequential[ms]", "batched[ms]", "speedup"
    );
    for d in [64usize] {
        let g = Geometry::new(
            (0..d)
                .map(|i| {
                    Autd3::new(
                        Point3::new(i as f32 * 200.0, 0.0, 0.0),
                        UnitQuaternion::identity(),
                    )
                })
                .collect(),
        );
        for nf in [1usize, 4] {
            for problems in [16usize, 64] {
                let owned: Vec<Vec<ControlPoint>> = (0..problems)
                    .map(|k| {
                        (0..nf)
                            .map(|i| ControlPoint {
                                point: g.center() + Vector3::new((k + i) as f32 * 7.0, 0.0, 150.0),
                                amplitude: 5e3 * Pa,
                            })
                            .collect()
                    })
                    .collect();
                let foci: Vec<ControlPoint> = owned.concat();
                let mut dst =
                    vec![vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; d]; problems];

                for (name, alg) in [("naive", 0u8), ("gs", 1), ("gspat", 2)] {
                    let mut seq_ms = 0.0;
                    let mut bat_ms = 0.0;
                    for (batched, acc) in [(false, &mut seq_ms), (true, &mut bat_ms)] {
                        run_batch(batched, alg, &gpu, &g, &foci, wl, &mut dst);
                        let t = Instant::now();
                        run_batch(batched, alg, &gpu, &g, &foci, wl, &mut dst);
                        *acc = t.elapsed().as_secs_f64() * 1e3;
                    }
                    println!(
                        "{name:<8} {d:>4} {nf:>5} {problems:>9} {seq_ms:>14.2} {bat_ms:>13.2} \
                         {:>7.2}x",
                        seq_ms / bat_ms
                    );
                }
            }
        }
    }
}

fn run_batch(
    batched: bool,
    alg: u8,
    gpu: &WgpuBackend,
    g: &Geometry,
    foci: &[ControlPoint],
    wl: autd3_rs_core::common::Length,
    dst: &mut [Vec<Vec<Emission>>],
) {
    if batched {
        match alg {
            0 => naive_batch(gpu, g, foci, wl, &NaiveOption::default(), dst).unwrap(),
            1 => gs_batch(gpu, g, foci, wl, &GsOption::default(), dst).unwrap(),
            _ => gspat_batch(gpu, g, foci, wl, &GspatOption::default(), dst).unwrap(),
        }
    } else {
        for (f, dst) in foci.chunks(foci.len() / dst.len()).zip(dst) {
            match alg {
                0 => naive(gpu, g, f, wl, &NaiveOption::default(), dst).unwrap(),
                1 => gs(gpu, g, f, wl, &GsOption::default(), dst).unwrap(),
                _ => gspat(gpu, g, f, wl, &GspatOption::default(), dst).unwrap(),
            }
        }
    }
}

fn call(
    tag: u8,
    run: u8,
    gpu: &WgpuBackend,
    g: &Geometry,
    f: &[ControlPoint],
    wl: autd3_rs_core::common::Length,
    dst: &mut [Vec<Emission>],
) {
    match (tag, run) {
        (0, 0) => naive(&NalgebraBackend, g, f, wl, &NaiveOption::default(), dst).unwrap(),
        (0, 1) => gs(&NalgebraBackend, g, f, wl, &GsOption::default(), dst).unwrap(),
        (0, _) => gspat(&NalgebraBackend, g, f, wl, &GspatOption::default(), dst).unwrap(),
        (_, 0) => naive(gpu, g, f, wl, &NaiveOption::default(), dst).unwrap(),
        (_, 1) => gs(gpu, g, f, wl, &GsOption::default(), dst).unwrap(),
        (_, _) => gspat(gpu, g, f, wl, &GspatOption::default(), dst).unwrap(),
    }
}
