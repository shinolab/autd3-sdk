use autd3_rs::commands::{
    ChangeModulationBank, ConfigModulation, Modulation, WriteModulationBuffer,
};
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};

fn main() {
    let bank = ModulationBank::B0;
    let config = SamplingConfig::FREQ_4K;
    let loop_behavior = LoopBehavior::Infinite;
    let transition_mode = TransitionMode::Immediate;

    let data = autd3_rs_modulation::modulation_buffer();

    // ANCHOR: api
    Modulation::new(config, &data);

    Modulation::with_bank(bank, config, &data);

    Modulation {
        bank,
        config,
        data: &data,
        loop_behavior,
        transition_mode,
    };
    // ANCHOR_END: api

    // ANCHOR: equivalent
    WriteModulationBuffer {
        bank,
        offset: 0,
        data: &data,
    };
    ConfigModulation {
        bank,
        config,
        size: data.len(),
        loop_behavior,
    };
    ChangeModulationBank {
        bank,
        transition_mode,
    };
    // ANCHOR_END: equivalent
}
