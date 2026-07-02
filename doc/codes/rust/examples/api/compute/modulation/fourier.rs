use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs_modulation::{FourierOption, SineComponent, SineOption, fourier};

fn main() -> Result<()> {
    let option =
        // ANCHOR: option
        FourierOption {
            scale_factor: None,
            clamp: false,
            offset: 0x00,
        }
        // ANCHOR_END: option
        ;
    let mut out = Vec::new();

    // Shown standalone in the SineComponent section of the docs.
    // ANCHOR: components
    SineComponent {
        freq: 100 * Hz,
        option: SineOption::default(),
    };
    // ANCHOR_END: components

    let components = [SineComponent {
        freq: 100 * Hz,
        option: SineOption::default(),
    }];
    // ANCHOR: api
    fourier(&components, &option, &mut out)?;
    // ANCHOR_END: api
    Ok(())
}
