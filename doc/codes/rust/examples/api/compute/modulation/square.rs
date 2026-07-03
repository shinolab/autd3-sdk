use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SquareOption, square};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let option =
        // ANCHOR: option
        SquareOption {
            low: u8::MIN,
            high: u8::MAX,
            duty: 0.5,
            sampling_config: SamplingConfig::FREQ_4K,
        }
        // ANCHOR_END: option
        ;
    let mut dst = Vec::new();
    // ANCHOR: api
    square(freq, &option, &mut dst)?;
    // ANCHOR_END: api
    Ok(())
}
