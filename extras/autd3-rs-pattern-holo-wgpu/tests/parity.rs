use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::{Emission, Intensity};
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, GsOption, GspatOption, NaiveOption,
    NalgebraBackend, Pa, TransducerMask, gs, gspat, naive,
};
use autd3_rs_pattern_holo_wgpu::WgpuBackend;

const CONSTRAINTS: [EmissionConstraint; 4] = [
    EmissionConstraint::Normalize,
    EmissionConstraint::Multiply(0.7),
    EmissionConstraint::Uniform(Intensity(0x80)),
    EmissionConstraint::Clamp(Intensity(16), Intensity(240)),
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

fn foci(g: &Geometry, n: usize) -> Vec<ControlPoint> {
    (0..n)
        .map(|i| ControlPoint {
            point: g.center() + Vector3::new(i as f32 * 10.0, i as f32 * -5.0, 150.0),
            amplitude: (3e3 + i as f32 * 200.0) * Pa,
        })
        .collect()
}

fn buffer(g: &Geometry) -> Vec<Vec<Emission>> {
    vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; g.num_devices()]
}

fn compare(label: &str, cpu: &[Vec<Emission>], gpu: &[Vec<Emission>]) {
    let mut worst_phase = 0u8;
    let mut worst_intensity = 0u8;
    for (d, (c, g)) in cpu.iter().zip(gpu).enumerate() {
        for (t, (a, b)) in c.iter().zip(g).enumerate() {
            let dp = a.phase.0.wrapping_sub(b.phase.0);
            let dp = dp.min(0u8.wrapping_sub(dp));
            let di = a.intensity.0.abs_diff(b.intensity.0);
            worst_phase = worst_phase.max(dp);
            worst_intensity = worst_intensity.max(di);
            assert!(
                dp <= 1,
                "{label}: phase mismatch at device {d} transducer {t}: {:?} vs {:?}",
                a.phase,
                b.phase
            );
            assert!(
                di <= 1,
                "{label}: intensity mismatch at device {d} transducer {t}: {:?} vs {:?}",
                a.intensity,
                b.intensity
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
                EmissionConstraint::Normalize,
                EmissionConstraint::Clamp(Intensity(16), Intensity(240)),
            ] {
                let label = format!("{devices}dev/{n}foci/{constraint:?}");
                let mut a = buffer(&g);
                let mut b = buffer(&g);

                let opt = NaiveOption {
                    constraint,
                    ..Default::default()
                };
                naive(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                naive(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                compare(&format!("naive {label}"), &a, &b);

                let opt = GsOption {
                    constraint,
                    ..Default::default()
                };
                gs(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                gs(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                compare(&format!("gs {label}"), &a, &b);

                let opt = GspatOption {
                    constraint,
                    ..Default::default()
                };
                gspat(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                gspat(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                compare(&format!("gspat {label}"), &a, &b);
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
                        naive(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                        naive(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                        compare(&format!("naive {label}"), &a, &b);

                        let opt = GsOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gs(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                        gs(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                        compare(&format!("gs {label}"), &a, &b);

                        let opt = GspatOption {
                            constraint,
                            directivity,
                            mask,
                            ..Default::default()
                        };
                        gspat(&NalgebraBackend, &g, &f, wl, &opt, &mut a).unwrap();
                        gspat(&gpu, &g, &f, wl, &opt, &mut b).unwrap();
                        compare(&format!("gspat {label}"), &a, &b);
                    }
                }
            }
        }
    }
}
