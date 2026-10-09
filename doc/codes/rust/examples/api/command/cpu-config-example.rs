use std::time::Duration;

use autd3_rs::commands::{CpuConfig, SetCpuConfig};
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

    client.send(SetCpuConfig::new(CpuConfig {
        sys_time_transition_margin: Duration::from_millis(20),
        ..Default::default()
    })).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
