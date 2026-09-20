use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{deg, m, mm, s};
use autd3_rs_pattern::{bessel, wavelength};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut phases = geometry.phase_buffer();

    bessel(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        Vector3::z_axis(),
        18.0 * deg,
        wavelength(340.0 * m / s),
        &mut phases,
    );
    // HIDE
}
// HIDE_END
