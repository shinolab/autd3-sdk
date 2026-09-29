use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, Interface};
use autd3_rs::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let iface = Interface::Auto;
    let heartbeat = Duration::from_millis(10);
    let reply_timeout = Duration::from_millis(1);
    let lost_timeout = Duration::from_millis(100);
    let response_timeout = Duration::from_millis(200);
    let enumeration_timeout = Duration::from_secs(10);
    let sync_timeout = Duration::from_secs(5);
    // ANCHOR: api
    TransportOption {
        iface,
        heartbeat,
        reply_timeout,
        lost_timeout,
        response_timeout,
        enumeration_timeout,
        sync_timeout,
        ..Default::default()
    };
    // ANCHOR_END: api

    // ANCHOR: iface
    Interface::Auto;
    Interface::Name("eth0".to_string());
    // ANCHOR_END: iface

    let _ = Client::open(&geometry, TransportOption::default(), ClientConfig::default());

    Ok(())
}
