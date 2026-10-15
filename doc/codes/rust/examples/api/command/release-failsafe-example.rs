use autd3_rs::commands::ReleaseFailsafe;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::UdpOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = UdpOption::for_emulator(emulator.interface());
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let states = client.read_fpga_state().await?;
    if states.iter().any(|state| state.is_failsafe_active()) {
        client.send(ReleaseFailsafe).await?;
    }

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
