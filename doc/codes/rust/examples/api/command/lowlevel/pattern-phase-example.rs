use autd3_rs::commands::{
    ActivatePatternBank, ConfigPattern, PhaseDepth, StmConfig, WritePatternPhase,
};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_pattern::{focus, wavelength};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let wavelength = wavelength(340.0 * m / s);
    let patterns = [-30.0f32, -10.0, 10.0, 30.0]
        .iter()
        .map(|&x| {
            let mut buffer = geometry.phase_buffer();
            focus(
                &geometry,
                geometry.center() + offset(x * mm, 0.0 * mm, 150.0 * mm),
                wavelength,
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();

    let bank = PatternBank::B0;

    client.send(WritePatternPhase {
        bank,
        index: 0,
        depth: PhaseDepth::Bits4,
        intensity: Intensity::MAX,
        patterns: &patterns,
    }).await?;
    client.send(ConfigPattern {
        bank,
        config: StmConfig::new(1.0 * Hz).into_sampling_config(patterns.len()),
        size: patterns.len(),
        loop_behavior: LoopBehavior::Infinite,
    }).await?;
    client.send(ActivatePatternBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
