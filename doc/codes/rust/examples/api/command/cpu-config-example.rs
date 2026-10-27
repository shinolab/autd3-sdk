use std::time::Duration;

use autd3_rs::commands::{Clear, CpuConfig, SetCpuConfig, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;

    client.send(SetCpuConfig::new(CpuConfig {
        ptp_unlock_failsafe_timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    })).await?;

    client.close()?;
    // HIDE
    Ok(())
}
// HIDE_END
