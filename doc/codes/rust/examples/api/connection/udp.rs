use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::Interface;
use autd3_rs::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let iface = Interface::Auto;
    let open_timeout = Duration::from_secs(40);
    let lost_timeout = Duration::from_millis(100);
    let ack_timeout = Duration::from_millis(10);
    let send_buffer_bytes = NonZeroUsize::new(8192);
    let sync_on_open = true;
    // ANCHOR: api
    TransportOption {
        iface,
        open_timeout,
        lost_timeout,
        ack_timeout,
        send_buffer_bytes,
        sync_on_open,
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
