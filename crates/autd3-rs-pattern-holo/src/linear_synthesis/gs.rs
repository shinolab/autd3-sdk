use core::num::NonZeroUsize;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::value::{Emission, Intensity};

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::linear_synthesis::batch::{BatchSetup, solve_batched};
use crate::mask::TransducerMask;
use crate::propagation::{make_propagation_matrix, quantize, target_amplitudes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GsOption<'a> {
    pub repeat: NonZeroUsize,
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

impl Default for GsOption<'_> {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
            parallel: true,
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn gs<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &GsOption<'_>,
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
    let amps = target_amplitudes(backend, foci);

    let n = mask.num_enabled(geometry);
    let q0 = backend.make_vector(vec![Complex::new(1.0, 0.0); n]);
    let mut q = backend.clone_vector(&q0);

    for _ in 0..option.repeat.get() {
        let p = backend.gemv_hadamard_normalized(&g, q, &q0);
        q = backend.gemv_hadamard_normalized(&b, p, &amps);
    }

    quantize(
        backend,
        geometry,
        &q,
        option.constraint,
        mask,
        option.parallel,
        dst,
    );
    Ok(())
}

pub fn gs_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &GsOption<'_>,
    dst: &mut [Vec<Vec<Emission>>],
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
        dst,
        |backend, g, amps, batch, n| {
            let b = backend.batch_back_prop(g);
            let q0 = backend.make_batch_vector(1, vec![Complex::new(1.0, 0.0); n]);
            let mut q = backend.make_batch_vector(batch, vec![Complex::new(1.0, 0.0); batch * n]);
            for _ in 0..option.repeat.get() {
                let p = backend.batch_gemv_hadamard_normalized(g, q, &q0);
                q = backend.batch_gemv_hadamard_normalized(&b, p, amps);
            }
            q
        },
    )
}
