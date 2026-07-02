use std::net::IpAddr;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_twincat::{AmsNetId, TwinCATLinkOption};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let addr: IpAddr = "169.254.0.1".parse()?;
    let ams_net_id = AmsNetId::from([169, 254, 0, 1, 1, 1]);
    // ANCHOR: api
    TwinCATLinkOption::local();

    TwinCATLinkOption::remote(addr, ams_net_id);
    // ANCHOR_END: api

    Ok(())
}
