use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{deg, m, mm, s};
use autd3_rs_pattern::{bessel, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let apex = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let direction = Vector3::z_axis();
    let theta = 18.0 * deg;
    let wavelength = wavelength(340.0 * m / s);
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: api
    bessel(&geometry, apex, direction, theta, wavelength, &mut dst);
    // ANCHOR_END: api
}
