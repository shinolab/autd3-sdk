use autd3_rs::commands::SetSilencer;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::{Client, ClientConfig, Driver};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let (mut driver, connector) = Driver::open(&emulator.option(), geometry.num_devices())?;
    std::thread::spawn(move || driver.run());
    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;

    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default());
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    let (phases, intensities) = emulator.with_device(0, |device| device.fpga().emissions());
    println!("{} transducers", phases.len().min(intensities.len()));

    client.close().await?;

    emulator.reboot(1);
    Ok(())
}
