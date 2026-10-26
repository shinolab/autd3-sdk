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
pub struct NaiveOption<'a> {
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
}

impl Default for NaiveOption<'_> {
    fn default() -> Self {
        Self {
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
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
            backend.gemv(&b, amps)
        },
    )
}
