use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs_modulation::{FourierOption, SineComponent, SineOption, fourier};

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
        }
        // ANCHOR_END: option
        ;
    let mut dst = Vec::new();

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
    fourier(&components, &option, &mut dst)?;
    // ANCHOR_END: api
    Ok(())
}
