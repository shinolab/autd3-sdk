use std::num::NonZeroUsize;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, TransducerMask, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, IntensityConstraint, GspatOption, NalgebraBackend, Pa, gspat,
};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = [
        AmplitudeTarget {
            point: center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
        AmplitudeTarget {
            point: center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
    ];

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let repeat = NonZeroUsize::new(100).unwrap();
    let constraint = IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX);
    let directivity = Directivity::Sphere;
    let mask = TransducerMask::AllEnabled;
    let parallel = true;
    let option =
        // ANCHOR: option
        GspatOption {
            repeat,
            constraint,
            directivity,
            mask,
            parallel,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();
    // ANCHOR: api
    gspat(
        &NalgebraBackend,
        &geometry,
        &foci,
        wavelength,
        &option,
        &mut phases,
        &mut intensities,
    )?;
    // ANCHOR_END: api
    Ok(())
}
