use std::time::Duration;

use autd3_rs::commands::{CpuConfig, SetCpuConfig};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::UdpOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = UdpOption::default();
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
