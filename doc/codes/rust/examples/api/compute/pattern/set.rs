use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{add_phase, set_intensity, set_phase, set_phase_and_intensity};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let intensity = Intensity(0x80);
    let phase = Phase::PI;
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: set_intensity
    set_intensity(intensity, &mut dst);
    // ANCHOR_END: set_intensity

    // ANCHOR: set_phase
    set_phase(phase, &mut dst);
    // ANCHOR_END: set_phase

    // ANCHOR: set_phase_and_intensity
    set_phase_and_intensity(phase, intensity, &mut dst);
    // ANCHOR_END: set_phase_and_intensity

    // ANCHOR: add_phase
    add_phase(phase, &mut dst);
    // ANCHOR_END: add_phase
}
