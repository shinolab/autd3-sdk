use autd3_rs_modulation::constant;

fn main() {
    let mut dst = Vec::new();
    let intensity = 0xFF;
    // ANCHOR: api
    constant(intensity, &mut dst);
    // ANCHOR_END: api
}
