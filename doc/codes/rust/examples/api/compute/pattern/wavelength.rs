use autd3_rs::units::{m, s};
use autd3_rs_pattern::wavelength;

fn main() {
    // ANCHOR: api
    let wavelength = wavelength(340.0 * m / s);
    // ANCHOR_END: api
    let _ = wavelength;
}
