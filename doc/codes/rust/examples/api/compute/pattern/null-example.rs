use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_pattern::null;

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    null(&mut dst);
    // HIDE
}
// HIDE_END
