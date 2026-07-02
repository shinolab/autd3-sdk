use autd3_rs::commands::SetPhaseCorrection;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::Phase;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let phases: Vec<Vec<Phase>> = geometry
        .iter()
        .map(|dev| vec![Phase::ZERO; dev.num_transducers()])
        .collect();

    // ANCHOR: api
    SetPhaseCorrection { phases: &phases };
    // ANCHOR_END: api
}
