use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption};
use autd3_rs_link_remote::{DeviceLayout, RemoteLinkError, RemoteServer, RemoteServerOption};

fn main() -> Result<()> {
    let bind: SocketAddr = "0.0.0.0:8080".parse()?;
    // ANCHOR: api
    let option = RemoteServerOption::new(bind);
    RemoteServer::new(option, |_: &[DeviceLayout]| {
        EchocatLink::open(&EchocatLinkOption::default())
            .map_err(|e| RemoteLinkError::Link(e.to_string()))
    })?
    .serve()?;
    // ANCHOR_END: api

    Ok(())
}
