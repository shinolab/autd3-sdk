use anyhow::Result;

use std::time::Duration;

use autd3_rs::Client;
use autd3_rs::commands::{Clear, Nop, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    // ANCHOR: open
    let mut client = Client::open(&geometry, &option).await?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;
    // ANCHOR_END: open

    // ANCHOR: api
    let num_devices = client.num_devices();
    let now = client.device_time_now()?;

    let firmware = client.read_firmware_version().await?;

    client.send(Nop).await?;
    client.send_streaming(Nop).await?;

    let state = tokio::time::timeout(Duration::from_millis(200), client.read_state()).await??;
    let fpga_state = state.replies().iter().map(|r| r.fpga_state).collect::<Vec<_>>();
    // ANCHOR_END: api

    let _ = (num_devices, now, firmware, fpga_state);
    Ok(())
}
