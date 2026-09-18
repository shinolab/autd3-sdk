use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{twin_trap, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let normal = Vector3::x_axis();
    let wavelength = wavelength(340.0 * m / s);
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: api
    twin_trap(&geometry, target, normal, wavelength, &mut dst);
    // ANCHOR_END: api
}
