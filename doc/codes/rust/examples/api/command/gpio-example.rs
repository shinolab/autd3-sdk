use autd3_rs::commands::{GpioOut, SetGpioOut};
use autd3_rs::geometry::{Autd3, Geometry};
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

    client.send(SetGpioOut {
        outputs: [
            GpioOut::PatternBank,
            GpioOut::Thermo,
            GpioOut::PwmOut(0),
            GpioOut::Off,
        ],
    }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
