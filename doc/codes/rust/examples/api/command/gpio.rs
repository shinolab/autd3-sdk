use autd3_rs::commands::{GpioOut, SetGpioOut};

fn main() {
    let gpio0 = GpioOut::PatternBank;
    let gpio1 = GpioOut::Thermo;
    let gpio2 = GpioOut::PwmOut(0);
    let gpio3 = GpioOut::Off;
    let outputs = [gpio0, gpio1, gpio2, gpio3];
    // ANCHOR: api
    SetGpioOut { outputs };
    // ANCHOR_END: api
}
