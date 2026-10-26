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
        let dir = dev.axial_direction();
        for (t, &pos) in dev.positions().iter().enumerate() {
            if mask.is_enabled(d, t) {
                tr_pos.push(pos);
                tr_dir.push(dir);
            }
        }
    }
    (tr_pos, tr_dir)
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
) -> B::Vector {
    backend.make_vector(
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

pub(crate) fn quantize<B: LinAlgBackend>(
    backend: &B,
    q: &B::Vector,
    constraint: IntensityConstraint,
    mask: TransducerMask<'_>,
    enabled: usize,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) {
    let (p, i) = backend.quantize(q, constraint);
    debug_assert_eq!(
        p.len(),
        enabled,
        "backend must return one phase per enabled transducer"
    );
    debug_assert_eq!(p.len(), i.len());
    scatter(&p, mask, Phase::ZERO, phases);
    scatter(&i, mask, Intensity::MIN, intensities);
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
                slot[..n].copy_from_slice(&e[at..][..n]);
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
