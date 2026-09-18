use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{add_phase, focus, set_intensity, wavelength};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    set_intensity(Intensity(0x80), &mut dst);
    focus(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        wavelength(340.0 * m / s),
        &mut dst,
    );
    add_phase(Phase::PI, &mut dst);

    set_intensity(Intensity::MIN, &mut dst);
    // HIDE
}
// HIDE_END
