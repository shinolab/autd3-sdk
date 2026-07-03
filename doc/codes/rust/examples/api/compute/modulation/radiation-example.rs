use autd3_rs::units::Hz;
use autd3_rs_modulation::{SineOption, radiation_pressure, sine};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
    let mut src = Vec::new();
    sine(150 * Hz, &SineOption::default(), &mut src)?;

    let mut dst = Vec::new();

    radiation_pressure(&src, &mut dst);

    // HIDE
    Ok(())
}
// HIDE_END
