#![cfg(test)]

use nalgebra::Complex;

use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};

use crate::amp::Pa;
use crate::amplitude_target::AmplitudeTarget;
use crate::backend::{LinAlgBackend, NalgebraBackend, hadamard_normalize};
use crate::directivity::Directivity;
use crate::mask::TransducerMask;
use crate::propagation::{enabled_transducers, target_amplitudes, wavenumber};

fn setup(devices: usize, nf: usize) -> (Geometry, Vec<AmplitudeTarget>) {
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
        .map(|i| AmplitudeTarget {
            point: g.center() + Vector3::new(i as f32 * 10.0, i as f32 * -5.0, 150.0),
            amplitude: (3e3 + i as f32 * 200.0) * Pa,
        })
        .collect();
    (g, f)
}

fn bits(v: &[nalgebra::DVector<Complex<f32>>]) -> Vec<(u32, u32)> {
    v.iter()
        .flatten()
        .map(|c| (c.re.to_bits(), c.im.to_bits()))
        .collect()
}

type Matrices = <NalgebraBackend as LinAlgBackend>::Matrix;
type Vectors = <NalgebraBackend as LinAlgBackend>::Vector;

fn problem(geo: &Geometry, foci: &[AmplitudeTarget]) -> (Matrices, Matrices, Vectors) {
    let b = NalgebraBackend;
    let wl = autd3_rs_pattern::wavelength(340.0 * m / s);
    let (tr_pos, tr_dir) = enabled_transducers(geo, TransducerMask::AllEnabled);
    let g = b.propagation_matrix(
        &tr_pos,
        &tr_dir,
        foci,
        1,
        wavenumber(wl),
        Directivity::Sphere,
    );
    let bp = b.back_prop(&g);
    (g, bp, target_amplitudes(&b, foci, 1))
}

fn normalized(mut x: Vectors, r: &Vectors) -> Vectors {
    hadamard_normalize(&mut x[0], &r[0]);
    x
}

#[test]
fn fused_gs_is_bit_identical() {
    let b = NalgebraBackend;
    for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
        let (geo, foci) = setup(devices, nf);
        let (g, bp, amps) = problem(&geo, &foci);
        let n = TransducerMask::AllEnabled.num_enabled(&geo);
        let q0 = b.make_vector(1, vec![Complex::new(1.0, 0.0); n]);

        let mut want = q0.clone();
        for _ in 0..repeat {
            let p = normalized(b.gemv(&g, &normalized(want, &q0)), &amps);
            want = b.gemv(&bp, &p);
        }

        let mut got = q0.clone();
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
    for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
        let (geo, foci) = setup(devices, nf);
        let (g, bp, amps) = problem(&geo, &foci);
        let r = b.gemm(&g, &bp);

        let mut zeta = amps.clone();
        let mut want = amps.clone();
        for _ in 0..repeat {
            want = b.gemv(&r, &zeta);
            zeta = normalized(want.clone(), &amps);
        }
        b.amplitude_correct(&mut want, &amps);
        let want = b.gemv(&bp, &want);

        let mut got = b.repeat_gemv_normalized(&r, b.gemv(&r, &amps), &amps, repeat - 1);
        b.amplitude_correct(&mut got, &amps);
        let got = b.gemv(&bp, &got);

        assert_eq!(
            bits(&want),
            bits(&got),
            "gspat {devices}dev/{nf}foci/{repeat}"
        );
    }
}
