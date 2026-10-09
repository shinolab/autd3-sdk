use autd3_rs::commands::{PatternStm, PatternStmOption, PhaseDepth};
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

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);
    let patterns = (0..200)
        .map(|i| {
            let theta = 2.0 * std::f32::consts::PI * i as f32 / 200.0;
            let target =
                center + offset(30.0 * theta.cos() * mm, 30.0 * theta.sin() * mm, 0.0 * mm);
            let mut buffer = geometry.phase_buffer();
            focus(
                &geometry,
                target,
                wavelength,
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();
    client.send(PatternStm::new(
        1.0 * Hz,
        &patterns,
        Intensity::MAX,
        PatternStmOption {
            bank: PatternBank::B0,
            phase_depth: PhaseDepth::Bits8,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
            ..Default::default()
        },
    )).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
