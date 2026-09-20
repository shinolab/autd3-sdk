use core::f32::consts::PI;

use autd3_rs::commands::Pattern;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, rad, s};
use autd3_rs::value::Phase;
use autd3_rs_pattern::wavelength;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);

    // ANCHOR: api
    let mut phases = geometry.phase_buffer();
    for (slot, device) in phases.iter_mut().zip(&geometry) {
        for (p, &pos) in slot.iter_mut().zip(device.positions()) {
            let dist = (target - pos).norm();
            *p = Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad);
        }
    }
    let intensities = geometry.intensity_buffer();

    Pattern::new(&phases, &intensities);
    // ANCHOR_END: api
}
