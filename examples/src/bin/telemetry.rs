// Poll every firmware telemetry counter and print what changed.
//
// Pass an address to reach an appliance over the Remote Link; without one it drives the local
// EtherCAT interface directly.
//
// Run with: cargo xtask example telemetry

use std::net::SocketAddrV6;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::{Client, ClientConfig, Driver, Telemetry, TelemetryCounters, TransportOption};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption {
        group: std::env::args()
            .nth(1)
            .map(|addr| addr.parse::<SocketAddrV6>())
            .transpose()?,
        ..TransportOption::default()
    };
    let (mut driver, connector) = Driver::open(&option, geometry.num_devices())?;
    std::thread::spawn(move || driver.run());
    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;

    println!("devices: {}", client.num_devices());
    let mut baseline: Option<Vec<TelemetryCounters>> = None;
    println!("polling telemetry — press Ctrl+C to stop");
    loop {
        let snapshot = client.read_telemetry().await?;
        match baseline.replace(snapshot) {
            None => print_snapshot(baseline.as_ref().expect("just stored")),
            Some(before) => print_deltas(&before, baseline.as_ref().expect("just stored")),
        }
        tokio::select! {
            () = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    client.close().await?;
    Ok(())
}

fn print_snapshot(snapshot: &[TelemetryCounters]) {
    for counter in Telemetry::ALL {
        let values: Vec<u32> = snapshot.iter().map(|c| c.get(*counter)).collect();
        println!("{counter:?}: {values:?}");
    }
}

fn print_deltas(before: &[TelemetryCounters], after: &[TelemetryCounters]) {
    for counter in Telemetry::ALL {
        let deltas: Vec<u32> = before
            .iter()
            .zip(after)
            .map(|(b, a)| a.get(*counter).wrapping_sub(b.get(*counter)))
            .collect();
        if deltas.iter().any(|delta| *delta != 0) {
            println!("{counter:?}: +{deltas:?}");
        }
    }
}
