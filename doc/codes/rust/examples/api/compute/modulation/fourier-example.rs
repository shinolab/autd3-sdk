use autd3_rs::units::Hz;
use autd3_rs_modulation::{FourierOption, SineComponent, SineOption, fourier};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
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

    // HIDE
    Ok(())
}
// HIDE_END
