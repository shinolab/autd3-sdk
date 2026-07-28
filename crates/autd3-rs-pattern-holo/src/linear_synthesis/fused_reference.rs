#![cfg(test)]

use core::num::NonZeroUsize;

use nalgebra::Complex;

use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};

use crate::amp::Pa;
use crate::backend::{LinAlgBackend, NalgebraBackend};
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, target_amplitudes};

fn setup(devices: usize, nf: usize) -> (Geometry, Vec<ControlPoint>) {
    let g = Geometry::new(
        (0..devices)
            .map(|i| {
                Autd3::new(
                    Point3::new(i as f32 * 200.0, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    );
    let f = (0..nf)
        .map(|i| ControlPoint {
            point: g.center() + Vector3::new(i as f32 * 10.0, i as f32 * -5.0, 150.0),
            amplitude: (3e3 + i as f32 * 200.0) * Pa,
        })
        .collect();
    (g, f)
}

fn bits(v: &nalgebra::DVector<Complex<f32>>) -> Vec<(u32, u32)> {
    v.iter().map(|c| (c.re.to_bits(), c.im.to_bits())).collect()
}

#[test]
fn fused_gs_is_bit_identical() {
    let b = NalgebraBackend;
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);
    for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
        let (geo, foci) = setup(devices, nf);
        let g = make_propagation_matrix(
            &b,
            &geo,
            &foci,
            wl,
            Directivity::Sphere,
            TransducerMask::AllEnabled,
        );
        let bp = b.back_prop(&g);
        let amps = target_amplitudes(&b, &foci);
        let n = TransducerMask::AllEnabled.num_enabled(&geo);
        let q0 = b.make_vector(vec![Complex::new(1.0, 0.0); n]);

        let mut want = b.clone_vector(&q0);
        for _ in 0..repeat {
            b.hadamard_normalize(&mut want, &q0);
            let mut p = b.gemv(&g, &want);
            b.hadamard_normalize(&mut p, &amps);
            want = b.gemv(&bp, &p);
        }

        let mut got = b.clone_vector(&q0);
        for _ in 0..repeat {
            let p = b.gemv_hadamard_normalized(&g, got, &q0);
            got = b.gemv_hadamard_normalized(&bp, p, &amps);
        }

        assert_eq!(bits(&want), bits(&got), "gs {devices}dev/{nf}foci/{repeat}");
    }
}

#[test]
fn fused_gspat_is_bit_identical() {
    let b = NalgebraBackend;
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);
    for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
        let (geo, foci) = setup(devices, nf);
        let g = make_propagation_matrix(
            &b,
            &geo,
            &foci,
            wl,
            Directivity::Sphere,
            TransducerMask::AllEnabled,
        );
        let bp = b.back_prop(&g);
        let amps = target_amplitudes(&b, &foci);
        let r = b.gemm(&g, &bp);
        let repeat = NonZeroUsize::new(repeat).unwrap().get();

        let mut zeta = b.clone_vector(&amps);
        let mut want = b.clone_vector(&amps);
        for _ in 0..repeat {
            want = b.gemv(&r, &zeta);
            zeta = b.clone_vector(&want);
            b.hadamard_normalize(&mut zeta, &amps);
        }
        b.amplitude_correct(&mut want, &amps);
        let want = b.gemv(&bp, &want);

        let mut got = b.gemv(&r, &amps);
        for _ in 1..repeat {
            got = b.gemv_hadamard_normalized(&r, got, &amps);
        }
        b.amplitude_correct(&mut got, &amps);
        let got = b.gemv(&bp, &got);

        assert_eq!(
            bits(&want),
            bits(&got),
            "gspat {devices}dev/{nf}foci/{repeat}"
        );
    }
}
