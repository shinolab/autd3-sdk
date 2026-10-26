use core::num::NonZeroUsize;

use nalgebra::Complex;

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
pub struct GsOption<'a> {
    pub repeat: NonZeroUsize,
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
}

impl Default for GsOption<'_> {
    fn default() -> Self {
        Self {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
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
        |backend, g, amps, n| {
            let b = backend.back_prop(g);
            let q0 = backend.make_vector(vec![Complex::new(1.0, 0.0); n]);
            let mut q = backend.make_vector(vec![Complex::new(1.0, 0.0); n]);
            for _ in 0..option.repeat.get() {
                let p = backend.gemv_hadamard_normalized(g, q, &q0);
                q = backend.gemv_hadamard_normalized(&b, p, amps);
            }
            q
        },
    )
}
