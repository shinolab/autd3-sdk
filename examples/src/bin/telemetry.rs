// Poll every firmware telemetry counter and print what changed.
//
// Run with: cargo xtask rust example telemetry

use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::{Client, ClientConfig, Telemetry, TransportOption};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

type Read = fn(&Telemetry) -> u32;

const COUNTERS: [(&str, Read); 11] = [
    ("fifo_drop", |t| t.fifo_drop),
    ("dedup", |t| t.dedup),
    ("seq_mismatch", |t| t.seq_mismatch),
    ("dispatch_error", |t| t.dispatch_error),
    ("processed", |t| t.processed),
    ("send_failure", |t| t.send_failure),
    ("failsafe", |t| u32::from(t.failsafe)),
    ("ptp_unlock_failsafe", |t| u32::from(t.ptp_unlock_failsafe)),
    ("sync_resync", |t| u32::from(t.sync_resync)),
    ("boot_failure", |t| u32::from(t.boot_failure)),
    ("cpu_watchdog", |t| u32::from(t.cpu_watchdog)),
];

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

    println!("devices: {}", client.num_devices());
    let mut baseline: Option<Vec<Telemetry>> = None;
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

fn print_snapshot(snapshot: &[Telemetry]) {
    for (name, read) in COUNTERS {
        let values: Vec<u32> = snapshot.iter().map(read).collect();
        println!("{name}: {values:?}");
    }
}

fn print_deltas(before: &[Telemetry], after: &[Telemetry]) {
    for (name, read) in COUNTERS {
        let deltas: Vec<u32> = before
            .iter()
            .zip(after)
            .map(|(b, a)| read(a).wrapping_sub(read(b)))
            .collect();
        if deltas.iter().any(|delta| *delta != 0) {
            println!("{name}: +{deltas:?}");
        }
    }
}
