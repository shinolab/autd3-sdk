use anyhow::Result;

use autd3_rs::commands::{ChangeModulationBank, ConfigModulation, WriteModulationBuffer};
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let bank = ModulationBank::B0;
    let offset = 0;
    let mut buffer = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut buffer)?;
    let data = &buffer;
    // ANCHOR: write
    WriteModulationBuffer { bank, offset, data };
    // ANCHOR_END: write
    let config = SamplingConfig::FREQ_4K;
    let size = data.len();
    let loop_behavior = LoopBehavior::Infinite;
    // ANCHOR: config
    ConfigModulation {
        bank,
        config,
        size,
        loop_behavior,
    };
    // ANCHOR_END: config
    let transition_mode = TransitionMode::Immediate;
    // ANCHOR: change
    ChangeModulationBank {
        bank,
        transition_mode,
    };
    // ANCHOR_END: change
    Ok(())
}
