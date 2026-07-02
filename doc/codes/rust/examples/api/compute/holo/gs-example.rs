use std::num::NonZeroUsize;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs_pattern::wavelength;
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, GsOption, NalgebraBackend, Pa, TransducerMask,
    gs,
};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = geometry.pattern_buffer();

    gs(
        &geometry,
        &[
            ControlPoint {
                point: geometry.center() + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
            ControlPoint {
                point: geometry.center() + offset(30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
        ],
        wavelength(340.0 * m / s),
        &GsOption {
            repeat: NonZeroUsize::new(100).unwrap(),
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            backend: NalgebraBackend,
            mask: TransducerMask::AllEnabled,
        },
        &mut out,
    )?;

    Ok(())
}
