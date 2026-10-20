use autd3_rs::commands::{ActivateModulationBank, ConfigModulation, WriteModulationBuffer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::UdpOption;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = UdpOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let mut data = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut data)?;

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
