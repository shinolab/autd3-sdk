use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs_modulation::{SineOption, modulation_buffer, radiation_pressure, sine};

fn main() -> Result<()> {
    let mut src = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut src)?;

    let mut dst = modulation_buffer();
    // ANCHOR: api
    radiation_pressure(&src, &mut dst);
    // ANCHOR_END: api
    Ok(())
}
