use autd3_rs_modulation::{constant, modulation_buffer};

// HIDE
fn main() {
    // HIDE_END
    let mut dst = modulation_buffer();

    constant(0xFF, &mut dst);
    // HIDE
}
// HIDE_END
