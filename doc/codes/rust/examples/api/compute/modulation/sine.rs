use anyhow::Result;

use autd3_rs::units::{Hz, rad};
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{Nearest, SineOption, sine};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let option =
        // ANCHOR: option
        SineOption {
            amplitude: 0xFF,
            offset: 0x80,
            phase: 0.0 * rad,
            clamp: false,
            sampling_config: SamplingConfig::FREQ_4K,
        }
        // ANCHOR_END: option
        ;
    let mut out = Vec::new();
    // ANCHOR: api
    sine(freq, &option, &mut out)?;
    // ANCHOR_END: api

    // ANCHOR: nearest
    sine(Nearest(150.5 * Hz), &option, &mut out)?;
    // ANCHOR_END: nearest
    Ok(())
}
