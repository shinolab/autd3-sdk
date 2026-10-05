use core::num::NonZeroUsize;

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
pub struct GspatOption<'a> {
    pub repeat: NonZeroUsize,
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

impl Default for GspatOption<'_> {
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

pub fn gspat<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GspatOption<'_>,
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

pub fn gspat_batch<B: LinAlgBackend>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GspatOption<'_>,
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
    option: &GspatOption<'_>,
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
            let r = backend.gemm(g, &b);
            let mut gamma = backend.repeat_gemv_normalized(
                &r,
                backend.gemv(&r, amps),
                amps,
                option.repeat.get() - 1,
            );
            backend.amplitude_correct(&mut gamma, amps);
            backend.gemv(&b, &gamma)
        },
    )
}
