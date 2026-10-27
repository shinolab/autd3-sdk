use std::time::Duration;

use autd3_rs::commands::{CpuConfig, ReleaseFailsafe, SetCpuConfig, each};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceStatus};
use autd3_rs::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, &TransportOption::default())?;

    // ANCHOR: ptp_failsafe
    let status = client.device_status()?;
    let releasable = |s: &DeviceStatus| s.failsafe_active && s.ptp_locked;
    if status.iter().any(releasable) {
        client
            .send(each(|device| {
                releasable(&status[device.idx()]).then_some(ReleaseFailsafe)
            }))
            .await?;
    }
    // ANCHOR_END: ptp_failsafe

    // ANCHOR: heartbeat
    client
        .send(SetCpuConfig::new(CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(100)),
            ..Default::default()
        }))
        .await?;

    let mut heartbeat = tokio::time::interval(Duration::from_millis(10));
    loop {
        heartbeat.tick().await;
        client.send_heartbeat()?;
    }
    // ANCHOR_END: heartbeat

    client.close()?;
    Ok(())
}
