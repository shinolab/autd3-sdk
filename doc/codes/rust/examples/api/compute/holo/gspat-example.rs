use std::num::NonZeroUsize;

use autd3_rs::geometry::{Autd3, Geometry, TransducerMask, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs_pattern::wavelength;
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, IntensityConstraint, GspatOption, NalgebraBackend, Pa, gspat,
};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();

    gspat(
        &NalgebraBackend,
        &geometry,
        &[
            AmplitudeTarget {
                point: geometry.center() + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
            AmplitudeTarget {
                point: geometry.center() + offset(30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
        ],
        wavelength(340.0 * m / s),
        &GspatOption {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: IntensityConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
            parallel: true,
            ..Default::default()
        },
        &mut phases,
        &mut intensities,
    )?;

    // HIDE
    Ok(())
}
// HIDE_END
