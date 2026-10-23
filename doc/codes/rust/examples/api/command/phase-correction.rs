use autd3_rs::commands::SetPhaseCorrection;
use autd3_rs::geometry::{Autd3, Geometry};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let phases = geometry.phase_buffer();

    // ANCHOR: api
    SetPhaseCorrection { phases: &phases };
    // ANCHOR_END: api
}
