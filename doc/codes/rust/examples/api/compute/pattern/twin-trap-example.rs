use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{TwinTrapOption, twin_trap, wavelength};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    twin_trap(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        Vector3::x_axis(),
        wavelength(340.0 * m / s),
        &TwinTrapOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        },
        &mut dst,
    );
    // HIDE
}
// HIDE_END
