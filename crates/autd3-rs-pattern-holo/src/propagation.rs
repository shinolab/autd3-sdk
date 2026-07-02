use core::f32::consts::PI;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::{Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::mask::TransducerMask;

const T4010A1_AMPLITUDE: f32 = 275.574_25 * 200.0;

#[must_use]
pub(crate) fn propagate(
    tr_pos: Point3<f32>,
    tr_dir: UnitVector3<f32>,
    target: Point3<f32>,
    wavenumber: f32,
    directivity: Directivity,
) -> Complex<f32> {
    const P0: f32 = T4010A1_AMPLITUDE / (4. * PI);
    let diff = target - tr_pos;
    let dist = diff.norm();
    let theta = tr_dir.cross(&diff).norm().atan2(tr_dir.dot(&diff));
    let r = P0 / dist * directivity.value(theta);
    let phase = wavenumber * dist;
    Complex::new(r * phase.cos(), r * phase.sin())
}

pub(crate) fn make_propagation_matrix<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    directivity: Directivity,
    mask: TransducerMask<'_>,
) -> B::Matrix {
    let wavenumber = 2.0 * PI / wavelength.mm();
    let m = foci.len();
    let n = mask.num_enabled(geometry);
    let mut data = Vec::with_capacity(m * n);
    for (d, dev) in geometry.iter().enumerate() {
        for (t, (&pos, &dir)) in dev.positions().iter().zip(dev.directions()).enumerate() {
            if mask.is_enabled(d, t) {
                for f in foci {
                    data.push(propagate(pos, dir, f.point, wavenumber, directivity));
                }
            }
        }
    }
    backend.make_matrix(m, n, data)
}

#[must_use]
pub(crate) fn target_amplitudes<B: LinAlgBackend>(backend: &B, foci: &[ControlPoint]) -> B::Vector {
    backend.make_vector(
        foci.iter()
            .map(|f| Complex::new(f.amplitude.pascal(), 0.0))
            .collect(),
    )
}

pub(crate) fn quantize(
    geometry: &Geometry,
    q: &[Complex<f32>],
    constraint: EmissionConstraint,
    mask: TransducerMask<'_>,
    out: &mut [Vec<Emission>],
) {
    assert_eq!(
        out.len(),
        geometry.num_devices(),
        "out must have one slot per device"
    );
    let max_coefficient = q
        .iter()
        .map(nalgebra::Complex::norm_sqr)
        .fold(0.0_f32, f32::max)
        .sqrt();
    let mut idx = 0;
    for (d, (slot, dev)) in out.iter_mut().zip(geometry.iter()).enumerate() {
        assert_eq!(
            dev.num_transducers(),
            Autd3::NUM_TRANSDUCERS,
            "not an AUTD3 device"
        );
        for (t, e) in slot.iter_mut().enumerate() {
            if mask.is_enabled(d, t) {
                let v = q[idx];
                idx += 1;
                *e = Emission {
                    phase: Phase::from(v),
                    intensity: constraint.convert(v.norm(), max_coefficient),
                };
            } else {
                *e = Emission::default();
            }
        }
    }
}
