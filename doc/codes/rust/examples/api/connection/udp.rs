use std::num::NonZeroUsize;

use anyhow::Result;

use autd3_rs::Interface;
use autd3_rs::TransportOption;
use autd3_rs::geometry::{Autd3, Geometry};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let iface = Interface::Auto;
    let send_buffer_bytes = NonZeroUsize::new(8192);
    let sync_on_open = true;
    let heartbeat_interval = None;
    // ANCHOR: api
    TransportOption {
        iface,
        send_buffer_bytes,
        sync_on_open,
        heartbeat_interval,
        ..Default::default()
    };
    // ANCHOR_END: api

    // ANCHOR: iface
    Interface::Auto;
    Interface::Name("eth0".to_string());
    Interface::Addr("[::1]:44336".parse()?);
    // ANCHOR_END: iface

    let _ = geometry;

    Ok(())
}
