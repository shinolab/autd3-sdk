use autd3_rs::commands::SetOutputMask;
use autd3_rs::geometry::{Autd3, Geometry};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let masks: Vec<Vec<bool>> = geometry
        .iter()
        .map(|dev| vec![true; dev.num_transducers()])
        .collect();

    // ANCHOR: api
    SetOutputMask { masks: &masks };
    // ANCHOR_END: api
}
