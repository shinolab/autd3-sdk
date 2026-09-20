use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs_pattern::{add_phase, set_intensity, set_phase};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let intensity = Intensity(0x80);
    let phase = Phase::PI;
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();

    // ANCHOR: set_intensity
    set_intensity(intensity, &mut intensities);
    // ANCHOR_END: set_intensity

    // ANCHOR: set_phase
    set_phase(phase, &mut phases);
    // ANCHOR_END: set_phase

    // ANCHOR: add_phase
    add_phase(phase, &mut phases);
    // ANCHOR_END: add_phase
}
