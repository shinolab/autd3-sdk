use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{VortexOption, vortex, wavelength};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    vortex(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        Vector3::z_axis(),
        1,
        wavelength(340.0 * m / s),
        &VortexOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        },
        &mut dst,
    );
    // HIDE
}
// HIDE_END
