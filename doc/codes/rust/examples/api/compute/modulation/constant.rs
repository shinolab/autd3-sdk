use autd3_rs_modulation::{constant, modulation_buffer};

fn main() {
    let mut dst = modulation_buffer();
    let amplitude = 0xFF;
    // ANCHOR: api
    constant(amplitude, &mut dst);
    // ANCHOR_END: api
}
