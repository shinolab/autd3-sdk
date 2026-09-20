use autd3_rs::geometry::{Autd3, Geometry, Vector3};
use autd3_rs::units::{m, s};
use autd3_rs_pattern::{plane, wavelength};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut phases = geometry.phase_buffer();

    plane(
        &geometry,
        Vector3::z_axis(),
        wavelength(340.0 * m / s),
        &mut phases,
    );
    // HIDE
}
// HIDE_END
