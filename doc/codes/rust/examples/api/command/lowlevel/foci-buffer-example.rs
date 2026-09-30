use autd3_rs::commands::{ChangePatternBank, ConfigFociStm, StmConfig, WriteFociBuffer, circle};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};
use autd3_rs::{Client, ClientConfig, Driver};
use autd3_rs::udp::emulator::UdpEmulator;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let (mut driver, connector) = Driver::open(&emulator.option(), geometry.num_devices())?;
    std::thread::spawn(move || driver.run());
    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;

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

    let mut builder = client.datagram_builder();
    builder.push(WriteFociBuffer {
        bank,
        index_offset: 0,
        points: &points,
    });
    builder.push(ConfigFociStm {
        bank,
        config: StmConfig::new(1.0 * Hz).into_sampling_config(points.len()),
        size: points.len(),
        num_foci: 1,
        sound_speed: 340.0 * m / s,
        loop_behavior: LoopBehavior::Infinite,
    });
    builder.push(ChangePatternBank {
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
