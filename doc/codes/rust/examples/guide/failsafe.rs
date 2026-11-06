use std::sync::Arc;
use std::time::Duration;

use autd3_rs::commands::{CpuConfig, ReleaseFailsafe, SetCpuConfig, each};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceStatus};
use autd3_rs::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: heartbeat
    let client = Arc::new(Client::open(&geometry, &TransportOption::default()).await?);
    let heartbeat = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10));
            loop {
                interval.tick().await;
                if let Err(e) = client.send_heartbeat().await {
                    eprintln!("sending a heartbeat failed: {e}");
                }
            }
        }
    });

    client
        .send(SetCpuConfig::new(CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(100)),
            ..Default::default()
        }))
        .await?;
    // ANCHOR_END: heartbeat

    // ANCHOR: ptp_failsafe
    let msg_id = client.send_heartbeat().await?;
    let status = client.collect(msg_id).await?.device_status();
    let releasable = |s: &DeviceStatus| s.failsafe_active && s.ptp_locked;
    if status.iter().any(releasable) {
        client
            .send(each(|device| {
                releasable(&status[device.idx()]).then_some(ReleaseFailsafe)
            }))
            .await?;
    }
    // ANCHOR_END: ptp_failsafe

    heartbeat.abort();
    Ok(())
}
