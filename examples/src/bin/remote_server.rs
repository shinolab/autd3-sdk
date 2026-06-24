// Remote Link server: drives a real EtherCAT link locally and relays tx/rx
// frames to a remote client over TCP. Run this on the host wired to the AUTD3
// devices. Run with: cargo xtask example remote_server

use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs_link_ethercrab::EtherCrabLinkOption;
use autd3_rs_link_remote::RemoteServer;

const BIND_ADDR: &str = "0.0.0.0:8080";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind: SocketAddr = BIND_ADDR.parse()?;
    let server = RemoteServer::open(bind, EtherCrabLinkOption::default()).await?;

    println!(
        "remote link server listening on {bind} (devices: {}) — press Ctrl+C to stop",
        server.num_devices()
    );

    let mut server = server;
    tokio::task::spawn_blocking(move || server.serve()).await??;
    Ok(())
}
