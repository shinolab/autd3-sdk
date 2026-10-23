use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SineOption, modulation_buffer, radiation_pressure_inplace, sine};

// HIDE
fn main() -> anyhow::Result<()> {
    // HIDE_END
    let mut buffer = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), SamplingConfig::FREQ_4K, &mut buffer)?;

    radiation_pressure_inplace(&mut buffer);

    // HIDE
    Ok(())
}
// HIDE_END
