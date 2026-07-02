use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_remote::RemoteLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;

    // ANCHOR: api
    RemoteLinkOption::new(addr);
    // ANCHOR_END: api

    Ok(())
}
