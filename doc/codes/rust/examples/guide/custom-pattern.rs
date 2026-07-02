use core::f32::consts::PI;

use autd3_rs::commands::Pattern;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, rad, s};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::wavelength;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);

    // ANCHOR: api
    let mut emissions = geometry.pattern_buffer();
    for (slot, device) in emissions.iter_mut().zip(&geometry) {
        for (e, &pos) in slot.iter_mut().zip(device.positions()) {
            let dist = (target - pos).norm();
            *e = Emission {
                phase: Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad),
                intensity: Intensity::MAX,
            };
        }
    }

    Pattern::new(&emissions);
    // ANCHOR_END: api
}
