use core::num::NonZeroUsize;

use nalgebra::Complex;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GsOption<'a> {
    pub repeat: NonZeroUsize,
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

impl Default for GsOption<'_> {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
            parallel: true,
        }
    }
}

pub fn gs<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GsOption<'_>,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), HoloError> {
    solve(
        backend,
        geometry,
        foci,
        wavelength,
        option,
        &mut [phases],
        &mut [intensities],
    )
}

pub fn gs_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GsOption<'_>,
    phases: &mut [Vec<Vec<Phase>>],
    intensities: &mut [Vec<Vec<Intensity>>],
) -> Result<(), HoloError> {
    solve(
        backend,
        geometry,
        foci,
        wavelength,
        option,
        phases,
        intensities,
    )
}

fn solve<B, P, I>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GsOption<'_>,
    phases: &mut [P],
    intensities: &mut [I],
) -> Result<(), HoloError>
where
    B: LinAlgBackend,
    P: AsMut<[Vec<Phase>]>,
    I: AsMut<[Vec<Intensity>]>,
{
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
        |backend, g, amps, batch, n| {
            let b = backend.back_prop(g);
            let q0 = backend.make_vector(1, vec![Complex::new(1.0, 0.0); n]);
            let mut q = backend.make_vector(batch, vec![Complex::new(1.0, 0.0); batch * n]);
            for _ in 0..option.repeat.get() {
                let p = backend.gemv_hadamard_normalized(g, q, &q0);
                q = backend.gemv_hadamard_normalized(&b, p, amps);
            }
            q
        },
    )
}
