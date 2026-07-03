use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs_modulation::{FourierOption, SineComponent, SineOption, fourier};

fn main() -> Result<()> {
    let mut dst = Vec::new();

    fourier(
        &[SineComponent {
            freq: 100 * Hz,
            option: SineOption::default(),
        }],
        &FourierOption {
            scale_factor: None,
            clamp: false,
            offset: 0x00,
        },
        &mut dst,
    )?;

    Ok(())
}
