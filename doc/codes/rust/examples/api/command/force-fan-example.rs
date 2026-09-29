use autd3_rs::commands::ForceFan;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::emulator::UdpEmulator;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let client = Client::open(&geometry, emulator.option(), ClientConfig::default()).await?;

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
