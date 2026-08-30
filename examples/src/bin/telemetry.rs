// Poll every firmware telemetry counter and print what changed.
//
// Pass an address to reach an appliance over the Remote Link; without one it drives the local
// EtherCAT interface directly.
//
// Run with: cargo xtask example telemetry

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::{Client, ClientConfig, Telemetry};
use autd3_rs_link_echocat::EchocatLinkOption;
use autd3_rs_link_remote::RemoteLinkOption;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = match std::env::args().nth(1) {
        Some(addr) => {
            Client::open(
                &geometry,
                RemoteLinkOption::new(addr.parse::<SocketAddr>()?),
                ClientConfig::default(),
            )
            .await?
        }
        None => {
            Client::open(
                &geometry,
                EchocatLinkOption::default(),
                ClientConfig::default(),
            )
            .await?
        }
    };

    println!("devices: {}", client.num_devices());
    let mut baseline: Option<Vec<Vec<u8>>> = None;
    println!("polling telemetry — press Ctrl+C to stop");
    loop {
        let mut snapshot = Vec::with_capacity(Telemetry::ALL.len());
        for counter in Telemetry::ALL {
            snapshot.push(client.read_telemetry(*counter).await?);
        }
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

fn print_snapshot(snapshot: &[Vec<u8>]) {
    for (counter, values) in Telemetry::ALL.iter().zip(snapshot) {
        println!("{counter:?}: {values:?}");
    }
}

fn print_deltas(before: &[Vec<u8>], after: &[Vec<u8>]) {
    for ((counter, before), after) in Telemetry::ALL.iter().zip(before).zip(after) {
        let deltas: Vec<u8> = before
            .iter()
            .zip(after)
            .map(|(b, a)| a.wrapping_sub(*b))
            .collect();
        if deltas.iter().any(|delta| *delta != 0) {
            println!("{counter:?}: +{deltas:?}");
        }
    }
}
