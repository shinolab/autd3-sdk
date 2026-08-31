use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SquareOption, square};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let low = u8::MIN;
    let high = u8::MAX;
    let duty = 0.5;
    let sampling_config = SamplingConfig::FREQ_4K;
    let option =
        // ANCHOR: option
        SquareOption {
            low,
            high,
            duty,
            sampling_config,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut dst = Vec::new();
    // ANCHOR: api
    square(freq, &option, &mut dst)?;
    // ANCHOR_END: api
    Ok(())
}
