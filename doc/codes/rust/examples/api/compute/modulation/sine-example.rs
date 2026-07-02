use anyhow::Result;

use autd3_rs::units::{Hz, rad};
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SineOption, sine};

fn main() -> Result<()> {
    let mut out = Vec::new();

    sine(
        150 * Hz,
        &SineOption {
            amplitude: 0xFF,
            offset: 0x80,
            phase: 0.0 * rad,
            clamp: false,
            sampling_config: SamplingConfig::FREQ_4K,
        },
        &mut out,
    )?;

    Ok(())
}
