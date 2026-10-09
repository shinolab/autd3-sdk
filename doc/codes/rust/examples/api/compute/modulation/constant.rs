use autd3_rs_modulation::constant;

fn main() {
    let mut dst = Vec::new();
    let amplitude = 0xFF;
    // ANCHOR: api
    constant(amplitude, &mut dst);
    // ANCHOR_END: api
}
