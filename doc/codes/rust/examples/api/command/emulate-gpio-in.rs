use autd3_rs::commands::EmulateGpioIn;

fn main() {
    let values = [true, false, false, false];
    // ANCHOR: api
    EmulateGpioIn { values };
    // ANCHOR_END: api
}
