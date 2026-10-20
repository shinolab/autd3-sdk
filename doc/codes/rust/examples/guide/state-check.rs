use std::time::Duration;

use autd3_rs::commands::{ReleaseFailsafe, Synchronize, each};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceStatus};
use autd3_rs::TransportOption;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: open
    let client = Client::open(&geometry, &TransportOption::default())?;
    let checker = client.state_checker();
    // ANCHOR_END: open

    // ANCHOR: recover
    if checker.check()?.any_lost() {
        let recovered = client.recover().await?;
        if !recovered.is_empty() {
            client
                .send(each(|device| {
                    recovered[device.idx()].then_some(Synchronize)
                }))
                .await?;
            client.send(ReleaseFailsafe).await?;
        }
    }
    // ANCHOR_END: recover

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
