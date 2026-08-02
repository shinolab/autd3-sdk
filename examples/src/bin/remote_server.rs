// Remote Link server: drives a real EtherCAT link locally and relays tx/rx frames to a remote client over TCP.
//
// Run with: cargo xtask example remote_server

use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption};
use autd3_rs_link_remote::{DeviceLayout, RemoteLinkError, RemoteServer, RemoteServerOption};

const BIND_ADDR: &str = "0.0.0.0:8080";

fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let bind: SocketAddr = BIND_ADDR.parse()?;
    let mut server = RemoteServer::new(RemoteServerOption::new(bind), |_: &[DeviceLayout]| {
        EchocatLink::open(&EchocatLinkOption::default())
            .map_err(|e| RemoteLinkError::Link(e.to_string()))
    })?;

    println!("remote link server listening on {bind} — press Ctrl+C to stop");
    server.serve()?;
    Ok(())
}
