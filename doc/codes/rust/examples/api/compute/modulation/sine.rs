use anyhow::Result;

use autd3_rs::units::{Hz, rad};
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{Nearest, SineOption, modulation_buffer, sine};

fn main() -> Result<()> {
    let freq = 150 * Hz;
    let amplitude = 0xFF;
    let offset = 0x80;
    let phase = 0.0 * rad;
    let clamp = false;
    let option =
        // ANCHOR: option
        SineOption {
            amplitude,
            offset,
            phase,
            clamp,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let sampling_config = SamplingConfig::FREQ_4K;
    let mut dst = modulation_buffer();
    // ANCHOR: api
    sine(freq, &option, sampling_config, &mut dst)?;
    // ANCHOR_END: api

    // ANCHOR: nearest
    sine(Nearest(150.5 * Hz), &option, sampling_config, &mut dst)?;
    // ANCHOR_END: nearest
    Ok(())
}
