use autd3_rs::commands::{ActivatePatternBank, ConfigFociStm, StmConfig, WriteFociBuffer, circle};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

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
    let mut points = Vec::new();
    circle(
        center,
        30.0 * mm,
        200,
        Vector3::z_axis(),
        Intensity::MAX,
        &mut points,
    );

    let bank = PatternBank::B0;

    client.send(WriteFociBuffer {
        bank,
        index_offset: 0,
        points: &points,
    }).await?;
    client.send(ConfigFociStm {
        bank,
        config: StmConfig::new(1.0 * Hz).into_sampling_config(points.len()),
        size: points.len(),
        num_foci: 1,
        sound_speed: 340.0 * m / s,
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
