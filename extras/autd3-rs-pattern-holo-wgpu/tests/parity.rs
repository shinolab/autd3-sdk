use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, TransducerMask, UnitQuaternion, Vector3};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, GsOption, GspatOption, IntensityConstraint, NaiveOption,
    NalgebraBackend, Pa, gs, gspat, naive,
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

fn foci(g: &Geometry, n: usize) -> Vec<AmplitudeTarget> {
    (0..n)
        .map(|i| AmplitudeTarget {
            point: g.center() + Vector3::new(i as f32 * 10.0, i as f32 * -5.0, 150.0),
            amplitude: (3e3 + i as f32 * 200.0) * Pa,
        })
        .collect()
}

type Buffers = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);

fn buffer(g: &Geometry) -> Buffers {
    (g.phase_buffer(), g.intensity_buffer())
}

fn compare(
    label: &str,
    cpu_phases: &[Vec<Phase>],
    cpu_intensities: &[Vec<Intensity>],
    gpu_phases: &[Vec<Phase>],
    gpu_intensities: &[Vec<Intensity>],
) {
    let mut worst_phase = 0u8;
    let mut worst_intensity = 0u8;
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
            worst_phase = worst_phase.max(dp);
            worst_intensity = worst_intensity.max(di);
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
    println!("{label}: max phase diff {worst_phase}, max intensity diff {worst_intensity}");
}

#[test]
fn multi_chunk_matches_nalgebra_backend() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);

    for devices in [9usize, 64] {
        let g = geometry(devices);
        for n in [1usize, 4, 16] {
            let f = foci(&g, n);
            for constraint in [
                IntensityConstraint::Normalize,
                IntensityConstraint::Clamp(Intensity(16), Intensity(240)),
            ] {
                let label = format!("{devices}dev/{n}foci/{constraint:?}");
                let mut a = buffer(&g);
                let mut b = buffer(&g);

                let opt = NaiveOption {
                    constraint,
                    ..Default::default()
                };
                naive(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                naive(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                compare(&format!("naive {label}"), &a.0, &a.1, &b.0, &b.1);

                let opt = GsOption {
                    constraint,
                    ..Default::default()
                };
                gs(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                gs(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                compare(&format!("gs {label}"), &a.0, &a.1, &b.0, &b.1);

                let opt = GspatOption {
                    constraint,
                    ..Default::default()
                };
                gspat(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                gspat(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                compare(&format!("gspat {label}"), &a.0, &a.1, &b.0, &b.1);
            }
        }
    }
}

#[test]
fn matches_nalgebra_backend() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);

    for devices in [1usize, 4] {
        let g = geometry(devices);
        let mut enabled = vec![vec![true; Autd3::NUM_TRANSDUCERS]; devices];
        for (t, slot) in enabled[0].iter_mut().enumerate() {
            *slot = t % 3 != 0;
        }
        for n in [1usize, 4, 16] {
            let f = foci(&g, n);
            for directivity in [Directivity::Sphere, Directivity::T4010A1] {
                for mask in [TransducerMask::AllEnabled, TransducerMask::Masked(&enabled)] {
                    for constraint in CONSTRAINTS {
                        let label =
                            format!("{devices}dev/{n}foci/{directivity:?}/{constraint:?}/{mask:?}");

                        let mut a = buffer(&g);
                        let mut b = buffer(&g);
                        let opt = NaiveOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        naive(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                        naive(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                        compare(&format!("naive {label}"), &a.0, &a.1, &b.0, &b.1);

                        let opt = GsOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gs(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                        gs(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                        compare(&format!("gs {label}"), &a.0, &a.1, &b.0, &b.1);

                        let opt = GspatOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gspat(&NalgebraBackend, &g, &f, wl, &opt, &mut a.0, &mut a.1).unwrap();
                        gspat(&gpu, &g, &f, wl, &opt, &mut b.0, &mut b.1).unwrap();
                        compare(&format!("gspat {label}"), &a.0, &a.1, &b.0, &b.1);
                    }
                }
            }
        }
    }
}
