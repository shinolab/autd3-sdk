use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::{Client, ClientConfig, Driver};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;

    let (mut driver, connector) = Driver::open(&emulator.option(), geometry.num_devices())?;
    let driver = std::thread::spawn(move || driver.run());

    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;
    client.close().await?;

    driver.join().unwrap()?;
    Ok(())
}
