use autd3_rs::commands::{ChangeModulationBank, ConfigModulation, WriteModulationBuffer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs::{Client, ClientConfig, Driver};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let (mut driver, connector) = Driver::open(&emulator.option(), geometry.num_devices())?;
    std::thread::spawn(move || driver.run());
    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;

    let mut data = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut data)?;

    let bank = ModulationBank::B0;

    let mut builder = client.datagram_builder();
    builder.push(WriteModulationBuffer {
        bank,
        offset: 0,
        data: &data,
    });
    builder.push(ConfigModulation {
        bank,
        config: SamplingConfig::FREQ_4K,
        size: data.len(),
        loop_behavior: LoopBehavior::Infinite,
    });
    builder.push(ChangeModulationBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    });
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
