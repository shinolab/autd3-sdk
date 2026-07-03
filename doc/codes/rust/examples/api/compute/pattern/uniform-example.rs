use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::uniform;

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    uniform(
        Emission {
            phase: Phase::ZERO,
            intensity: Intensity::MAX,
        },
        &mut dst,
    );
    // HIDE
}
// HIDE_END
