use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::Emission;
use autd3_rs_pattern::null;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    // ANCHOR: api
    null(&mut out);
    // ANCHOR_END: api
}
