use autd3_rs_modulation::constant;

fn main() {
    let mut out = Vec::new();
    let intensity = 0xFF;
    // ANCHOR: api
    constant(intensity, &mut out);
    // ANCHOR_END: api
}
