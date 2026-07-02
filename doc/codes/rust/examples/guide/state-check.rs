use std::time::Duration;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, LinkStatus};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: open
    let (client, mut checker) = Client::open_with_checker(
        &geometry,
        EtherCrabLinkOption::default(),
        ClientConfig::default(),
    )
    .await?;
    // ANCHOR_END: open

    // ANCHOR: poll
    let mut last: Option<LinkStatus> = None;
    loop {
        let status = checker.check().await?;
        if last.as_ref() != Some(&status) {
            for (i, state) in status.devices.iter().enumerate() {
                println!("device[{i}]: {state}");
            }
            println!(
                "all operational: {}, any lost: {}, recoveries: {}",
                status.all_op(),
                status.any_lost(),
                status.recoveries
            );
            last = Some(status);
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
    // ANCHOR_END: poll

    client.close().await?;
    Ok(())
}
