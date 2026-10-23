use std::time::Duration;

use autd3_rs::commands::{ReleaseFailsafe, each};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceState, DeviceStatus};
use autd3_rs::TransportOption;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: open
    let client = Client::open(&geometry, &TransportOption::default())?;
    // ANCHOR_END: open

    // ANCHOR: ptp_failsafe
    client.send_heartbeat()?;
    tokio::time::sleep(CHECK_INTERVAL).await;
    let status = client.device_status()?;
    let releasable = |s: &DeviceStatus| s.ptp_failsafe && s.ptp_locked;
    if status.iter().any(releasable) {
        client
            .send(each(|device| {
                releasable(&status[device.idx()]).then_some(ReleaseFailsafe)
            }))
            .await?;
    }
    // ANCHOR_END: ptp_failsafe

    // ANCHOR: poll
    let mut last: Vec<DeviceStatus> = Vec::new();
    loop {
        client.send_heartbeat()?;
        let status = client.device_status()?;
        if last != status {
            for (i, s) in status.iter().enumerate() {
                println!("device[{i}]: {}", s.state);
            }
            println!(
                "all ready: {}, any lost: {}",
                status.iter().all(|s| s.state == DeviceState::Ready),
                status.iter().any(|s| s.state == DeviceState::Lost)
            );
            last = status;
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
    // ANCHOR_END: poll

    client.close()?;
    Ok(())
}
