use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_remote::{DiscoveryOption, RemoteLinkOption};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let timeout = Some(std::time::Duration::from_secs(1));

    // ANCHOR: api
    RemoteLinkOption { addr, timeout };
    // ANCHOR_END: api

    // ANCHOR: discover
    RemoteLinkOption::discover()?;
    // ANCHOR_END: discover

    let timeout = std::time::Duration::from_secs(1);
    // ANCHOR: discover_option
    RemoteLinkOption::discover_with(&DiscoveryOption {
        timeout,
        instance: Some("autd3-0a1b2c3d".to_string()),
        ..Default::default()
    })?;
    // ANCHOR_END: discover_option

    // ANCHOR: discover_list
    for appliance in autd3_rs_link_remote::discover_all(&DiscoveryOption::default())? {
        println!("{} at {}", appliance.instance, appliance.addr);
    }
    // ANCHOR_END: discover_list

    Ok(())
}
