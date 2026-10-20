// Watch the link status for every device by driving the state checker.
//
// Run with: cargo xtask rust example status_check

use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{Clear, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, DeviceStatus, TransportOption};
use tracing_subscriber::EnvFilter;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, &TransportOption::default())?;
    client.send((Clear, Synchronize)).await?;
    let checker = client.state_checker();

    println!("watching device status — press Ctrl+C to stop");
    let mut last: Option<DeviceStatus> = None;
    loop {
        client.send_heartbeat()?;
        let status = checker.check()?;
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

fn print_status(status: &DeviceStatus) {
    for (i, state) in status.devices().iter().enumerate() {
        println!("device[{i}]: {state}");
    }
    println!(
        "all ready: {}, any lost: {}",
        status.all_ready(),
        status.any_lost()
    );
}
