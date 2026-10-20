use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SquareOption, modulation_buffer, square};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let low = u8::MIN;
    let high = u8::MAX;
    let duty = 0.5;
    let option =
        // ANCHOR: option
        SquareOption {
            low,
            high,
            duty,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let sampling_config = SamplingConfig::FREQ_4K;
    let mut dst = modulation_buffer();
    // ANCHOR: api
    square(freq, &option, sampling_config, &mut dst)?;
    // ANCHOR_END: api
    Ok(())
}
