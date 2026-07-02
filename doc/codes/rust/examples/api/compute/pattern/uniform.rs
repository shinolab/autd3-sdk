use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::uniform;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let emission = Emission {
        phase: Phase::ZERO,
        intensity: Intensity::MAX,
    };
    let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    // ANCHOR: api
    uniform(emission, &mut out);
    // ANCHOR_END: api
}
