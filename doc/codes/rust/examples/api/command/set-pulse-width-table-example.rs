use autd3_rs::commands::SetPulseWidthTable;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;
use autd3_rs::value::PulseWidth;
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

    let mut table = SetPulseWidthTable::empty_table();
    for (width, i) in table.iter_mut().zip(0..) {
        *width = PulseWidth::new(i);
    }

    client.send(SetPulseWidthTable { table: &table }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
