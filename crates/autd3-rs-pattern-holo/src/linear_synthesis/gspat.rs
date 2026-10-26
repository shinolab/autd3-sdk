use core::num::NonZeroUsize;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, TransducerMask};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::linear_synthesis::synthesis::{SynthesisSetup, synthesize};

#[derive(Debug, Clone, PartialEq)]
pub struct GspatOption<'a> {
    pub repeat: NonZeroUsize,
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
}

impl Default for GspatOption<'_> {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
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
    let setup = SynthesisSetup {
        constraint: option.constraint,
        directivity: option.directivity,
        mask: option.mask,
    };
    synthesize(
        backend,
        geometry,
        foci,
        wavelength,
        &setup,
        phases,
        intensities,
        |backend, g, amps, _| {
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
