use autd3_rs::commands::Modulation;
use autd3_rs::value::SamplingConfig;

fn main() {
    // ANCHOR: api
    let length = 10;
    let mut data = vec![0x00u8; length];
    data[0] = 0xFF;

    Modulation::new(SamplingConfig::FREQ_4K, &data);
    // ANCHOR_END: api
}
