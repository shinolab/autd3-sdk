use core::f32::consts::PI;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
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
    foci: &[AmplitudeTarget],
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
pub(crate) fn target_amplitudes<B: LinAlgBackend>(
    backend: &B,
    foci: &[AmplitudeTarget],
) -> B::Vector {
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

pub(crate) fn batch_shape(foci: &[AmplitudeTarget], problems: usize) -> Result<usize, HoloError> {
    if problems == 0 {
        return Err(HoloError::NoProblems);
    }
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    if !foci.len().is_multiple_of(problems) {
        return Err(HoloError::BatchSizeMismatch {
            foci: foci.len(),
            problems,
        });
    }
    Ok(foci.len() / problems)
}

#[must_use]
pub(crate) fn batch_target_amplitudes<B: LinAlgBackend>(
    backend: &B,
    foci: &[AmplitudeTarget],
    problems: usize,
) -> B::BatchVector {
    backend.make_batch_vector(
        problems,
        foci.iter()
            .map(|f| Complex::new(f.amplitude.pascal(), 0.0))
            .collect(),
    )
}

pub(crate) fn emission(
    v: Complex<f32>,
    constraint: IntensityConstraint,
    max_coefficient: f32,
) -> (Phase, Intensity) {
    (
        Phase::from(v),
        constraint.convert(v.norm(), max_coefficient),
    )
}

pub(crate) fn max_coefficient(q: &[Complex<f32>]) -> f32 {
    q.iter()
        .map(nalgebra::Complex::norm_sqr)
        .fold(0.0_f32, f32::max)
        .sqrt()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn quantize<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    q: &B::Vector,
    constraint: IntensityConstraint,
    mask: TransducerMask<'_>,
    parallel: bool,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) {
    assert_eq!(
        phases.len(),
        geometry.num_devices(),
        "phases must have one slot per device"
    );
    assert_eq!(
        intensities.len(),
        geometry.num_devices(),
        "intensities must have one slot per device"
    );
    let (p, i) = backend.quantize(q, constraint, parallel);
    scatter(&p, mask, Phase::ZERO, phases);
    scatter(&i, mask, Intensity::MIN, intensities);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn quantize_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    q: &B::BatchVector,
    constraint: IntensityConstraint,
    mask: TransducerMask<'_>,
    parallel: bool,
    phases: &mut [Vec<Vec<Phase>>],
    intensities: &mut [Vec<Vec<Intensity>>],
) {
    let n = mask.num_enabled(geometry);
    let devices = geometry.num_devices();
    assert_eq!(
        phases.len(),
        intensities.len(),
        "phases and intensities must have one entry per problem"
    );
    let (p, i) = backend.quantize_batch(q, constraint, parallel);
    debug_assert_eq!(
        p.len(),
        n * phases.len(),
        "backend must return one phase per enabled transducer per problem"
    );
    debug_assert_eq!(p.len(), i.len());
    for (k, (phases, intensities)) in phases.iter_mut().zip(intensities.iter_mut()).enumerate() {
        assert_eq!(
            phases.len(),
            devices,
            "phases must have one slot per device"
        );
        assert_eq!(
            intensities.len(),
            devices,
            "intensities must have one slot per device"
        );
        let range = n * k..n * (k + 1);
        scatter(&p[range.clone()], mask, Phase::ZERO, phases);
        scatter(&i[range], mask, Intensity::MIN, intensities);
    }
}

fn scatter<T: Copy>(e: &[T], mask: TransducerMask<'_>, null: T, dst: &mut [Vec<T>]) {
    match mask {
        TransducerMask::AllEnabled => {
            debug_assert_eq!(
                e.len(),
                dst.iter().map(Vec::len).sum::<usize>(),
                "backend must return one value per transducer"
            );
            let mut at = 0;
            for slot in dst {
                let n = slot.len().min(e.len() - at);
                slot[..n].copy_from_slice(&e[at..at + n]);
                at += n;
            }
        }
        mask => {
            let mut idx = 0;
            for (d, slot) in dst.iter_mut().enumerate() {
                for (t, out) in slot.iter_mut().enumerate() {
                    *out = if mask.is_enabled(d, t) {
                        idx += 1;
                        e[idx - 1]
                    } else {
                        null
                    };
                }
            }
        }
    }
}
