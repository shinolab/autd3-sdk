use core::f32::consts::PI;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, Point3, TransducerMask, UnitVector3};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;

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

pub(crate) fn wavenumber(wavelength: Length) -> f32 {
    2.0 * PI / wavelength.mm()
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

pub(crate) fn validate_dst_len(dst: usize, geometry: &Geometry) -> Result<(), HoloError> {
    if dst != geometry.num_devices() {
        return Err(HoloError::DstDeviceCountMismatch {
            got: dst,
            expected: geometry.num_devices(),
        });
    }
    Ok(())
}

#[must_use]
pub(crate) fn target_amplitudes<B: LinAlgBackend>(
    backend: &B,
    foci: &[AmplitudeTarget],
    problems: usize,
) -> B::Vector {
    backend.make_vector(
        problems,
        foci.iter()
            .map(|f| Complex::new(f.amplitude.pascal(), 0.0))
            .collect(),
    )
}

pub(crate) fn phase_and_intensity(
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
pub(crate) fn quantize<B, P, I>(
    backend: &B,
    geometry: &Geometry,
    q: &B::Vector,
    constraint: IntensityConstraint,
    mask: TransducerMask<'_>,
    parallel: bool,
    phases: &mut [P],
    intensities: &mut [I],
) where
    B: LinAlgBackend,
    P: AsMut<[Vec<Phase>]>,
    I: AsMut<[Vec<Intensity>]>,
{
    let n = mask.num_enabled(geometry);
    let devices = geometry.num_devices();
    assert_eq!(
        phases.len(),
        intensities.len(),
        "phases and intensities must have one entry per problem"
    );
    let (p, i) = backend.quantize(q, constraint, parallel);
    debug_assert_eq!(
        p.len(),
        n * phases.len(),
        "backend must return one phase per enabled transducer per problem"
    );
    debug_assert_eq!(p.len(), i.len());
    for (k, (phases, intensities)) in phases.iter_mut().zip(intensities.iter_mut()).enumerate() {
        let (phases, intensities) = (phases.as_mut(), intensities.as_mut());
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
