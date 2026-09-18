use autd3_rs::geometry::{Autd3, Geometry, Vector3};
use autd3_rs::units::{m, s};
use autd3_rs_pattern::{plane, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let direction = Vector3::z_axis();
    let wavelength = wavelength(340.0 * m / s);
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: api
    plane(&geometry, direction, wavelength, &mut dst);
    // ANCHOR_END: api
}
