// Watch the EtherCAT link status for every device by driving the state checker.
// Run with: cargo xtask example status_check

use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, LinkStatus};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![Autd3::default()]);

    let (client, mut checker) = Client::open_with_checker(
        &geometry,
        EtherCrabLinkOption::default(),
        ClientConfig::default(),
    )
    .await?;

    println!("watching link status — press Ctrl+C to stop");
    let mut last: Option<LinkStatus> = None;
    loop {
        let status = checker.check().await?;
        if last.as_ref() != Some(&status) {
            print_status(&status);
            last = Some(status);
        }
        tokio::select! {
            () = tokio::time::sleep(CHECK_INTERVAL) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    client.close().await?;
    Ok(())
}

fn print_status(status: &LinkStatus) {
    for (i, state) in status.devices.iter().enumerate() {
        println!("device[{i}]: {state}");
    }
    println!(
        "all operational: {}, any lost: {}, recoveries: {}",
        status.all_op(),
        status.any_lost(),
        status.recoveries
    );
}
