use std::time::Duration;

use autd3_rs::commands::{ReleaseFailsafe, each};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceMask, DeviceStatus};
use autd3_rs::TransportOption;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: open
    let client = Client::open(&geometry, &TransportOption::default())?;
    let checker = client.state_checker();
    // ANCHOR_END: open

    // ANCHOR: ptp_failsafe
    client.send_heartbeat()?;
    tokio::time::sleep(CHECK_INTERVAL).await;
    let status = checker.check()?;
    if status.any_ptp_failsafe() {
        let releasable: DeviceMask = status
            .ptp_failsafe()
            .iter()
            .filter(|&device| status.ptp_locked()[device])
            .collect();
        if !releasable.is_empty() {
            client
                .send(each(|device| {
                    releasable[device.idx()].then_some(ReleaseFailsafe)
                }))
                .await?;
        }
    }
    // ANCHOR_END: ptp_failsafe

    // ANCHOR: poll
    let mut last: Option<DeviceStatus> = None;
    loop {
        client.send_heartbeat()?;
        let status = checker.check()?;
        if last.as_ref() != Some(&status) {
            for (i, state) in status.devices().iter().enumerate() {
                println!("device[{i}]: {state}");
            }
            println!(
                "all ready: {}, any lost: {}",
                status.all_ready(),
                status.any_lost()
            );
            last = Some(status);
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
    // ANCHOR_END: poll

    client.close().await?;
    Ok(())
}
