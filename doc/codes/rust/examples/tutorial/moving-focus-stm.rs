use anyhow::Result;

use autd3_rs::commands::{FociStm, FociStmOption, Modulation, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{ControlPoints, SamplingConfig};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);

    // ANCHOR: stm
    let points = [
        ControlPoints::from(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm)),
        ControlPoints::from(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm)),
    ];
    client.send(FociStm::new(0.5 * Hz, &points, FociStmOption::default())).await?;
    // ANCHOR_END: stm

    client.close().await?;
    Ok(())
}
