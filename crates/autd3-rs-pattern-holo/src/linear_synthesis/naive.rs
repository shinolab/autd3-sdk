use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::linear_synthesis::batch::{BatchSetup, solve_batched};
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, quantize, target_amplitudes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NaiveOption<'a> {
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

impl Default for NaiveOption<'_> {
    fn default() -> Self {
        Self {
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
            parallel: true,
        }
    }
}

pub fn naive<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &NaiveOption<'_>,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    crate::mask::validate_dst_len(phases.len(), geometry)?;
    crate::mask::validate_dst_len(intensities.len(), geometry)?;
    let mask = option.mask;
    mask.validate(geometry)?;

    let g = make_propagation_matrix(
        backend,
        geometry,
        foci,
        wavelength,
        option.directivity,
        mask,
    );
    let b = backend.back_prop(&g);
    let p = target_amplitudes(backend, foci);
    let q = backend.gemv(&b, &p);

    quantize(
        backend,
        geometry,
        &q,
        option.constraint,
        mask,
        option.parallel,
        phases,
        intensities,
    );
    Ok(())
}

pub fn naive_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &NaiveOption<'_>,
    phases: &mut [Vec<Vec<Phase>>],
    intensities: &mut [Vec<Vec<Intensity>>],
) -> Result<(), HoloError> {
    let setup = BatchSetup {
        constraint: option.constraint,
        directivity: option.directivity,
        mask: option.mask,
        parallel: option.parallel,
    };
    solve_batched(
        backend,
        geometry,
        foci,
        wavelength,
        &setup,
        phases,
        intensities,
        |backend, g, amps, _, _| {
            let b = backend.batch_back_prop(g);
            backend.batch_gemv(&b, amps)
        },
    )
}
