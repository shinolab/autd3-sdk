use autd3_rs::commands::{
    ActivateModulationBank, Clear, ConfigModulation, Synchronize, WriteModulationBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send((Clear, Synchronize)).await?;

    let mut data = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), SamplingConfig::FREQ_4K, &mut data)?;

    let bank = ModulationBank::B0;

    client.send(WriteModulationBuffer {
        bank,
        offset: 0,
        data: &data,
    }).await?;
    client.send(ConfigModulation {
        bank,
        config: SamplingConfig::FREQ_4K,
        size: data.len(),
        loop_behavior: LoopBehavior::Infinite,
    }).await?;
    client.send(ActivateModulationBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
