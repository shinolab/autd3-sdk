// Watch the link status for every device by driving the state checker.
//
// Run with: cargo xtask rust example status_check

use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::{Client, ClientConfig, DeviceStatus, TransportOption};

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(
        &geometry,
        &TransportOption::default(),
        ClientConfig::default(),
    )
    .await?;
    let checker = client.state_checker();

    println!("watching device status — press Ctrl+C to stop");
    let mut last: Option<DeviceStatus> = None;
    loop {
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
