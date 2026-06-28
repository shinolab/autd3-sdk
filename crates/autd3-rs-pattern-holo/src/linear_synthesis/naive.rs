use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity};

use crate::backend::{LinAlgBackend, NalgebraBackend};
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, quantize, target_amplitudes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NaiveOption<'a, B = NalgebraBackend> {
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub backend: B,
    pub mask: TransducerMask<'a>,
}

impl Default for NaiveOption<'_, NalgebraBackend> {
    fn default() -> Self {
        Self {
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            backend: NalgebraBackend,
            mask: TransducerMask::AllEnabled,
        }
    }
}

pub fn naive<B: LinAlgBackend>(
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &NaiveOption<'_, B>,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    let backend = &option.backend;
    let mask = option.mask;
    mask.validate(geometry);

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
        geometry,
        &backend.vector_to_host(&q),
        option.constraint,
        mask,
        out,
    );
    Ok(())
}
