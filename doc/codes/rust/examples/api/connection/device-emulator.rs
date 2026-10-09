use autd3_rs::commands::SetSilencer;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs::{Client, ClientConfig};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    client.send(SetSilencer::default()).await?;

    let (phases, intensities) = emulator.with_device(0, |device| device.fpga().emissions());
    println!("{} transducers", phases.len().min(intensities.len()));

    client.close().await?;

    emulator.reboot(1);
    Ok(())
}
