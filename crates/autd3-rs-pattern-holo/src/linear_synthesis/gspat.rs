use core::num::NonZeroUsize;

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
pub struct GspatOption<'a, B = NalgebraBackend> {
    pub repeat: NonZeroUsize,
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub backend: B,
    pub mask: TransducerMask<'a>,
}

impl Default for GspatOption<'_, NalgebraBackend> {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            backend: NalgebraBackend,
            mask: TransducerMask::AllEnabled,
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn gspat<B: LinAlgBackend>(
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &GspatOption<'_, B>,
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
    let amps = target_amplitudes(backend, foci);

    let r = backend.gemm(&g, &b);

    let mut zeta = backend.clone_vector(&amps);
    let mut gamma = backend.clone_vector(&amps);
    for _ in 0..option.repeat.get() {
        gamma = backend.gemv(&r, &zeta);
        zeta = backend.clone_vector(&gamma);
        backend.hadamard_normalize(&mut zeta, &amps);
    }
    backend.amplitude_correct(&mut gamma, &amps);
    let q = backend.gemv(&b, &gamma);

    quantize(
        geometry,
        &backend.vector_to_host(&q),
        option.constraint,
        mask,
        out,
    );
    Ok(())
}
