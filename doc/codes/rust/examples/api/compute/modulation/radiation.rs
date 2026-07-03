use anyhow::Result;

use autd3_rs::units::Hz;
use autd3_rs_modulation::{SineOption, radiation_pressure, sine};

fn main() -> Result<()> {
    let mut src = Vec::new();
    sine(150 * Hz, &SineOption::default(), &mut src)?;

    let mut dst = Vec::new();
    // ANCHOR: api
    radiation_pressure(&src, &mut dst);
    // ANCHOR_END: api
    Ok(())
}
