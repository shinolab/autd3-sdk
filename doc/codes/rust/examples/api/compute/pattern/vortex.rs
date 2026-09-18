use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{vortex, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let axis = Vector3::z_axis();
    let order = 1;
    let wavelength = wavelength(340.0 * m / s);
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: api
    vortex(&geometry, target, axis, order, wavelength, &mut dst);
    // ANCHOR_END: api
}
