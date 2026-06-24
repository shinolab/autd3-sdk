use core::num::NonZeroUsize;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity};

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, quantize, target_amplitudes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GsOption {
    pub repeat: NonZeroUsize,
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
}

impl Default for GsOption {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn gs<B: LinAlgBackend>(
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &GsOption,
    backend: &B,
    mask: TransducerMask<'_>,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
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

    let n = mask.num_enabled(geometry);
    let q0 = backend.make_vector(vec![Complex::new(1.0, 0.0); n]);
    let mut q = backend.clone_vector(&q0);

    for _ in 0..option.repeat.get() {
        backend.hadamard_normalize(&mut q, &q0);
        let mut p = backend.gemv(&g, &q);
        backend.hadamard_normalize(&mut p, &amps);
        q = backend.gemv(&b, &p);
    }

    quantize(
        geometry,
        &backend.vector_to_host(&q),
        option.constraint,
        mask,
        out,
    );
    Ok(())
}
