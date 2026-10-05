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

pub fn naive_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &NaiveOption<'_>,
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
    option: &NaiveOption<'_>,
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
        |backend, g, amps, _, _| {
            let b = backend.back_prop(g);
            backend.gemv(&b, amps)
        },
    )
}
