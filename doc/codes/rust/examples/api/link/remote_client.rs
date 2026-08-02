use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_remote::{DiscoveryOption, RemoteLinkOption};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;

    // ANCHOR: api
    RemoteLinkOption::new(addr);
    // ANCHOR_END: api

    // ANCHOR: discover
    let option = RemoteLinkOption::discover()?;
    // ANCHOR_END: discover

    // ANCHOR: discover_option
    let option = RemoteLinkOption::discover_with(&DiscoveryOption {
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
