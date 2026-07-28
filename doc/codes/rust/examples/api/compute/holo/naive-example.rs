use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs_pattern::wavelength;
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, NaiveOption, NalgebraBackend, Pa,
    TransducerMask, naive,
};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    naive(
        &NalgebraBackend,
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
        &NaiveOption {
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            mask: TransducerMask::AllEnabled,
            parallel: true,
        },
        &mut dst,
    )?;

    // HIDE
    Ok(())
}
// HIDE_END
