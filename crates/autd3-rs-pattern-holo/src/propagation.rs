use core::f32::consts::PI;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
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
    let r = P0 / dist * directivity.value_at(tr_dir, &diff);
    let (sin, cos) = (wavenumber * dist).sin_cos();
    Complex::new(r * cos, r * sin)
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
    let (tr_pos, tr_dir) = enabled_transducers(geometry, mask);
    backend.propagation_matrix(&tr_pos, &tr_dir, foci, wavenumber, directivity)
}

pub(crate) fn wavenumber(wavelength: Length) -> f32 {
    2.0 * PI / wavelength.mm()
}

#[must_use]
pub(crate) fn target_amplitudes<B: LinAlgBackend>(backend: &B, foci: &[ControlPoint]) -> B::Vector {
    backend.make_vector(
        foci.iter()
            .map(|f| Complex::new(f.amplitude.pascal(), 0.0))
            .collect(),
    )
}

pub(crate) fn enabled_transducers(
    geometry: &Geometry,
    mask: TransducerMask<'_>,
) -> (Vec<Point3<f32>>, Vec<UnitVector3<f32>>) {
    let n = mask.num_enabled(geometry);
    let mut tr_pos = Vec::with_capacity(n);
    let mut tr_dir = Vec::with_capacity(n);
    for (d, dev) in geometry.iter().enumerate() {
        for (t, (&pos, &dir)) in dev.positions().iter().zip(dev.directions()).enumerate() {
            if mask.is_enabled(d, t) {
                tr_pos.push(pos);
                tr_dir.push(dir);
            }
        }
    }
    (tr_pos, tr_dir)
}

pub(crate) fn batch_shape(foci: &[&[ControlPoint]], slots: usize) -> Result<usize, HoloError> {
    if foci.len() != slots {
        return Err(HoloError::BatchSizeMismatch {
            problems: foci.len(),
            slots,
        });
    }
    let (first, rest) = foci.split_first().ok_or(HoloError::NoProblems)?;
    if first.is_empty() {
        return Err(HoloError::NoFoci);
    }
    if let Some(bad) = rest.iter().find(|f| f.len() != first.len()) {
        return Err(HoloError::UnevenBatch(first.len(), bad.len()));
    }
    Ok(first.len())
}

#[must_use]
pub(crate) fn batch_target_amplitudes<B: LinAlgBackend>(
    backend: &B,
    foci: &[&[ControlPoint]],
) -> B::BatchVector {
    backend.make_batch_vector(
        foci.len(),
        foci.iter()
            .flat_map(|f| f.iter())
            .map(|f| Complex::new(f.amplitude.pascal(), 0.0))
            .collect(),
    )
}

pub(crate) fn emission(
    v: Complex<f32>,
    constraint: EmissionConstraint,
    max_coefficient: f32,
) -> Emission {
    Emission {
        phase: Phase::from(v),
        intensity: constraint.convert(v.norm(), max_coefficient),
    }
}

pub(crate) fn max_coefficient(q: &[Complex<f32>]) -> f32 {
    q.iter()
        .map(nalgebra::Complex::norm_sqr)
        .fold(0.0_f32, f32::max)
        .sqrt()
}

pub(crate) fn quantize<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    q: &B::Vector,
    constraint: EmissionConstraint,
    mask: TransducerMask<'_>,
    parallel: bool,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    scatter(&backend.quantize(q, constraint, parallel), mask, dst);
}

pub(crate) fn quantize_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    q: &B::BatchVector,
    constraint: EmissionConstraint,
    mask: TransducerMask<'_>,
    parallel: bool,
    dst: &mut [Vec<Vec<Emission>>],
) {
    let n = mask.num_enabled(geometry);
    let devices = geometry.num_devices();
    let emissions = backend.quantize_batch(q, constraint, parallel);
    debug_assert_eq!(
        emissions.len(),
        n * dst.len(),
        "backend must return one emission per enabled transducer per problem"
    );
    if n == 0 {
        for dst in dst.iter_mut() {
            assert_eq!(dst.len(), devices, "dst must have one slot per device");
            scatter(&[], mask, dst);
        }
        return;
    }
    for (dst, e) in dst.iter_mut().zip(emissions.chunks(n)) {
        assert_eq!(dst.len(), devices, "dst must have one slot per device");
        scatter(e, mask, dst);
    }
}

fn scatter(e: &[Emission], mask: TransducerMask<'_>, dst: &mut [Vec<Emission>]) {
    match mask {
        TransducerMask::AllEnabled => {
            debug_assert_eq!(
                e.len(),
                dst.iter().map(Vec::len).sum::<usize>(),
                "backend must return one emission per transducer"
            );
            let mut at = 0;
            for slot in dst {
                let n = slot.len().min(e.len() - at);
                slot[..n].copy_from_slice(&e[at..at + n]);
                at += n;
            }
        }
        TransducerMask::Masked(m) => {
            let mut idx = 0;
            for (d, slot) in dst.iter_mut().enumerate() {
                for (t, out) in slot.iter_mut().enumerate() {
                    *out = if m[d][t] {
                        idx += 1;
                        e[idx - 1]
                    } else {
                        Emission::default()
                    };
                }
            }
        }
    }
}
