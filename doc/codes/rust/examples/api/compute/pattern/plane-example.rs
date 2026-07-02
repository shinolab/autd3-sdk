use autd3_rs::geometry::{Autd3, Geometry, UnitVector3, Vector3};
use autd3_rs::units::{m, s};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{PlaneOption, plane, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = geometry.pattern_buffer();

    plane(
        &geometry,
        UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0)),
        wavelength(340.0 * m / s),
        &PlaneOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        },
        &mut out,
    );
}
