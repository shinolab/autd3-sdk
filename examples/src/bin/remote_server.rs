// Remote Link server: drives a real EtherCAT link locally and relays tx/rx frames to a remote client over TCP.
//
// Run with: cargo xtask example remote_server

use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs_link_echocat::EchocatLinkOption;
use autd3_rs_link_remote::RemoteServer;

const BIND_ADDR: &str = "0.0.0.0:8080";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let bind: SocketAddr = BIND_ADDR.parse()?;
    let server = RemoteServer::open(bind, EchocatLinkOption::default()).await?;

    println!(
        "remote link server listening on {bind} (devices: {}) — press Ctrl+C to stop",
        server.num_devices()
    );

    let mut server = server;
    tokio::task::spawn_blocking(move || server.serve()).await??;
    Ok(())
}
