use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::value::{Emission, Intensity};

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, quantize, target_amplitudes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NaiveOption<'a> {
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
}

impl Default for NaiveOption<'_> {
    fn default() -> Self {
        Self {
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
        }
    }
}

pub fn naive<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &NaiveOption<'_>,
    dst: &mut [Vec<Emission>],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
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
        dst,
    );
    Ok(())
}
