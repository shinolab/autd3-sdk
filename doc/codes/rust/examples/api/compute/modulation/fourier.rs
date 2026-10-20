use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{fourier, modulation_buffer, FourierOption, SineComponent, SineOption};

fn main() -> Result<()> {
    let scale_factor = None;
    let clamp = false;
    let offset = 0x00;
    let option =
        // ANCHOR: option
        FourierOption {
            scale_factor,
            clamp,
            offset,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut dst = modulation_buffer();

    // ANCHOR: components
    SineComponent {
        freq: (100 * Hz).into(),
        option: SineOption::default(),
    };
    // ANCHOR_END: components

    let components = [SineComponent {
        freq: (100 * Hz).into(),
        option: SineOption::default(),
    }];
    let sampling_config = SamplingConfig::FREQ_4K;
    // ANCHOR: api
    fourier(&components, &option, sampling_config, &mut dst)?;
    // ANCHOR_END: api
    Ok(())
}
