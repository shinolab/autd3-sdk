use autd3_rs::units::Hz;
use autd3_rs_modulation::{SineOption, modulation_buffer, radiation_pressure, sine};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
    let mut src = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut src)?;

    let mut dst = modulation_buffer();

    radiation_pressure(&src, &mut dst);

    // HIDE
    Ok(())
}
// HIDE_END
