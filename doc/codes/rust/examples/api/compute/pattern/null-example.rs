use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_pattern::null;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = geometry.pattern_buffer();

    null(&mut out);
}
