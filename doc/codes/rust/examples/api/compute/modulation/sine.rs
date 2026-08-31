use anyhow::Result;

use autd3_rs::units::{Hz, rad};
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{Nearest, SineOption, sine};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let amplitude = 0xFF;
    let offset = 0x80;
    let phase = 0.0 * rad;
    let clamp = false;
    let sampling_config = SamplingConfig::FREQ_4K;
    let option =
        // ANCHOR: option
        SineOption {
            amplitude,
            offset,
            phase,
            clamp,
            sampling_config,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut dst = Vec::new();
    // ANCHOR: api
    sine(freq, &option, &mut dst)?;
    // ANCHOR_END: api

    // ANCHOR: nearest
    sine(Nearest(150.5 * Hz), &option, &mut dst)?;
    // ANCHOR_END: nearest
    Ok(())
}
