use core::f32::consts::PI;

use nalgebra::Complex;

use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{
    Autd3, Geometry, Point3, TransducerMask, UnitQuaternion, UnitVector3, Vector3,
};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, GsOption, GspatOption, IntensityConstraint, LinAlgBackend,
    NaiveOption, NalgebraBackend, Pa, gs, gs_batch, gspat, gspat_batch, naive, naive_batch,
};
use autd3_rs_pattern_holo_wgpu::WgpuBackend;

const CONSTRAINTS: [IntensityConstraint; 4] = [
    IntensityConstraint::Normalize,
    IntensityConstraint::Multiply(0.7),
    IntensityConstraint::Uniform(Intensity(0x80)),
    IntensityConstraint::Clamp(Intensity(16), Intensity(240)),
];

fn geometry(devices: usize) -> Geometry {
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

fn problem(g: &Geometry, seed: usize, nf: usize) -> Vec<AmplitudeTarget> {
    (0..nf)
        .map(|i| AmplitudeTarget {
            point: g.center()
                + Vector3::new(
                    (seed + i) as f32 * 7.0,
                    seed as f32 * -3.0,
                    120.0 + i as f32 * 5.0,
                ),
            amplitude: (3e3 + (seed * nf + i) as f32 * 100.0) * Pa,
        })
        .collect()
}

type Buffers = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);
type Batch = (Vec<Vec<Vec<Phase>>>, Vec<Vec<Vec<Intensity>>>);

fn slot(g: &Geometry) -> Buffers {
    (g.phase_buffer(), g.intensity_buffer())
}

fn batch(g: &Geometry, problems: usize) -> Batch {
    (
        vec![g.phase_buffer(); problems],
        vec![g.intensity_buffer(); problems],
    )
}

fn compare(
    label: &str,
    cpu_phases: &[Vec<Phase>],
    cpu_intensities: &[Vec<Intensity>],
    gpu_phases: &[Vec<Phase>],
    gpu_intensities: &[Vec<Intensity>],
) {
    for (d, ((cp, ci), (gp, gi))) in cpu_phases
        .iter()
        .zip(cpu_intensities)
        .zip(gpu_phases.iter().zip(gpu_intensities))
        .enumerate()
    {
        for (t, ((a, ai), (b, bi))) in cp.iter().zip(ci).zip(gp.iter().zip(gi)).enumerate() {
            let dp = a.0.wrapping_sub(b.0);
            let dp = dp.min(0u8.wrapping_sub(dp));
            let di = ai.0.abs_diff(bi.0);
            assert!(
                dp <= 1,
                "{label}: phase mismatch at device {d} transducer {t}: {a:?} vs {b:?}"
            );
            assert!(
                di <= 1,
                "{label}: intensity mismatch at device {d} transducer {t}: {ai:?} vs {bi:?}"
            );
        }
    }
}

fn transducers(g: &Geometry) -> (Vec<Point3<f32>>, Vec<UnitVector3<f32>>) {
    let mut pos = Vec::new();
    let mut dir = Vec::new();
    for dev in g {
        pos.extend_from_slice(dev.positions());
        dir.extend_from_slice(dev.directions());
    }
    (pos, dir)
}

fn seeded(len: usize, offset: usize) -> Vec<Complex<f32>> {
    (0..len)
        .map(|i| {
            let t = (offset + i) as f32 * 0.37;
            Complex::new(t.cos() * (1.0 + i as f32 * 0.01), t.sin())
        })
        .collect()
}

fn compare_values(label: &str, cpu: &[Complex<f32>], gpu: &[Complex<f32>]) {
    assert_eq!(cpu.len(), gpu.len(), "{label}: length mismatch");
    let scale = cpu.iter().map(|c| c.norm()).fold(0.0f32, f32::max);
    for (i, (a, b)) in cpu.iter().zip(gpu).enumerate() {
        assert!(
            (a - b).norm() <= 1e-3 * scale,
            "{label}: mismatch at {i}: {a} vs {b} (scale {scale})"
        );
    }
}

#[test]
fn broadcast_batch_operands_match_nalgebra() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let cpu = NalgebraBackend;

    for devices in [1usize, 2] {
        let g = geometry(devices);
        let (pos, dir) = transducers(&g);
        let n = pos.len();
        let k = 2.0 * PI / 8.5;

        for nf in [1usize, 4] {
            for problems in [2usize, 5] {
                let owned: Vec<Vec<AmplitudeTarget>> =
                    (0..problems).map(|p| problem(&g, p, nf)).collect();
                let flat: Vec<AmplitudeTarget> = owned.iter().flatten().copied().collect();
                let label = format!("{devices}dev/{nf}foci/{problems}problems");

                let a_host = cpu.batch_propagation_matrix(
                    &pos,
                    &dir,
                    &flat,
                    problems,
                    k,
                    Directivity::T4010A1,
                );
                let a_wgpu = gpu.batch_propagation_matrix(
                    &pos,
                    &dir,
                    &flat,
                    problems,
                    k,
                    Directivity::T4010A1,
                );
                let b_host = cpu.batch_back_prop(&a_host);
                let b_wgpu = gpu.batch_back_prop(&a_wgpu);

                let shared = seeded(n, 0);
                let p_host = cpu.batch_gemv(&a_host, &cpu.make_batch_vector(1, shared.clone()));
                let p_wgpu = gpu.batch_gemv(&a_wgpu, &gpu.make_batch_vector(1, shared.clone()));
                compare_values(
                    &format!("broadcast x gemv {label}"),
                    &cpu.batch_vector_to_host(&p_host),
                    &gpu.batch_vector_to_host(&p_wgpu),
                );

                let amps = seeded(nf, 11);
                let mut q_host = cpu.batch_gemv_hadamard_normalized(
                    &b_host,
                    p_host,
                    &cpu.make_batch_vector(1, amps.clone()),
                );
                let mut q_wgpu = gpu.batch_gemv_hadamard_normalized(
                    &b_wgpu,
                    p_wgpu,
                    &gpu.make_batch_vector(1, amps.clone()),
                );
                compare_values(
                    &format!("broadcast r gemv {label}"),
                    &cpu.batch_vector_to_host(&q_host),
                    &gpu.batch_vector_to_host(&q_wgpu),
                );

                let corr = seeded(n, 23);
                cpu.batch_amplitude_correct(&mut q_host, &cpu.make_batch_vector(1, corr.clone()));
                gpu.batch_amplitude_correct(&mut q_wgpu, &gpu.make_batch_vector(1, corr.clone()));
                compare_values(
                    &format!("broadcast r amplitude_correct {label}"),
                    &cpu.batch_vector_to_host(&q_host),
                    &gpu.batch_vector_to_host(&q_wgpu),
                );
            }
        }
    }
}

#[test]
fn batch_matches_nalgebra_on_pool_size_collision() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);

    let g = geometry(1);
    let nf = 1usize;
    let problems = Autd3::NUM_TRANSDUCERS * 2 / nf;
    let owned: Vec<Vec<AmplitudeTarget>> = (0..problems).map(|k| problem(&g, k, nf)).collect();
    let foci: Vec<AmplitudeTarget> = owned.concat();

    let mut batched = batch(&g, problems);
    let mut one = slot(&g);

    let opt = NaiveOption {
        constraint: IntensityConstraint::Normalize,
        directivity: Directivity::T4010A1,
        mask: TransducerMask::AllEnabled,
        ..Default::default()
    };
    naive_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1).unwrap();
    for (k, (f, got)) in owned
        .iter()
        .zip(batched.0.iter().zip(&batched.1))
        .enumerate()
    {
        naive(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1).unwrap();
        compare(
            &format!("naive {problems}problems #{k}"),
            &one.0,
            &one.1,
            got.0,
            got.1,
        );
    }

    let opt = GsOption {
        constraint: IntensityConstraint::Normalize,
        directivity: Directivity::T4010A1,
        mask: TransducerMask::AllEnabled,
        ..Default::default()
    };
    gs_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1).unwrap();
    for (k, (f, got)) in owned
        .iter()
        .zip(batched.0.iter().zip(&batched.1))
        .enumerate()
    {
        gs(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1).unwrap();
        compare(
            &format!("gs {problems}problems #{k}"),
            &one.0,
            &one.1,
            got.0,
            got.1,
        );
    }

    let opt = GspatOption {
        constraint: IntensityConstraint::Normalize,
        directivity: Directivity::T4010A1,
        mask: TransducerMask::AllEnabled,
        ..Default::default()
    };
    gspat_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1).unwrap();
    for (k, (f, got)) in owned
        .iter()
        .zip(batched.0.iter().zip(&batched.1))
        .enumerate()
    {
        gspat(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1).unwrap();
        compare(
            &format!("gspat {problems}problems #{k}"),
            &one.0,
            &one.1,
            got.0,
            got.1,
        );
    }
}

#[test]
fn batch_matches_nalgebra_per_problem() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);

    for devices in [1usize, 4] {
        let g = geometry(devices);
        let mut enabled = vec![vec![true; Autd3::NUM_TRANSDUCERS]; devices];
        for (t, on) in enabled[0].iter_mut().enumerate() {
            *on = t % 3 != 0;
        }
        for nf in [1usize, 4, 16] {
            for problems in [1usize, 2, 3, 9] {
                let owned: Vec<Vec<AmplitudeTarget>> =
                    (0..problems).map(|k| problem(&g, k, nf)).collect();
                let foci: Vec<AmplitudeTarget> = owned.concat();
                let directivity = Directivity::T4010A1;

                let mut batched = batch(&g, problems);
                let mut one = slot(&g);

                for mask in [TransducerMask::AllEnabled, TransducerMask::Masked(&enabled)] {
                    for constraint in CONSTRAINTS {
                        let label = format!(
                            "{devices}dev/{nf}foci/{problems}problems/{constraint:?}/{mask:?}"
                        );

                        let opt = NaiveOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        naive_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1)
                            .unwrap();
                        for (k, (f, got)) in owned
                            .iter()
                            .zip(batched.0.iter().zip(&batched.1))
                            .enumerate()
                        {
                            naive(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1)
                                .unwrap();
                            compare(&format!("naive {label} #{k}"), &one.0, &one.1, got.0, got.1);
                        }

                        let opt = GsOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gs_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1)
                            .unwrap();
                        for (k, (f, got)) in owned
                            .iter()
                            .zip(batched.0.iter().zip(&batched.1))
                            .enumerate()
                        {
                            gs(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1).unwrap();
                            compare(&format!("gs {label} #{k}"), &one.0, &one.1, got.0, got.1);
                        }

                        let opt = GspatOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gspat_batch(&gpu, &g, &foci, wl, &opt, &mut batched.0, &mut batched.1)
                            .unwrap();
                        for (k, (f, got)) in owned
                            .iter()
                            .zip(batched.0.iter().zip(&batched.1))
                            .enumerate()
                        {
                            gspat(&NalgebraBackend, &g, f, wl, &opt, &mut one.0, &mut one.1)
                                .unwrap();
                            compare(&format!("gspat {label} #{k}"), &one.0, &one.1, got.0, got.1);
                        }
                    }
                }
            }
        }
    }
}
