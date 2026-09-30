use autd3_rs::commands::ForceFan;
use autd3_rs::geometry::{Autd3, Geometry};
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

    let mut builder = client.datagram_builder();
    builder.push(ForceFan { value: true });
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
