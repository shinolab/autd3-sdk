use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SquareOption, square};

fn main() -> Result<()> {
    let mut out = Vec::new();

    square(
        150 * Hz,
        &SquareOption {
            low: u8::MIN,
            high: u8::MAX,
            duty: 0.5,
            sampling_config: SamplingConfig::FREQ_4K,
        },
        &mut out,
    )?;

    Ok(())
}
