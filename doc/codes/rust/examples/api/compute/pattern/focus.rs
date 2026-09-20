use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{focus, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();

    // ANCHOR: api
    focus(&geometry, target, wavelength, &mut phases);
    // ANCHOR_END: api
}
