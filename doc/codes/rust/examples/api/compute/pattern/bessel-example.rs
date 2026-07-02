use autd3_rs::geometry::{Autd3, Geometry, UnitVector3, Vector3, offset};
use autd3_rs::units::{deg, m, mm, s};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{BesselOption, bessel, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = geometry.pattern_buffer();

    bessel(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0)),
        18.0 * deg,
        wavelength(340.0 * m / s),
        &BesselOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        },
        &mut out,
    );
}
