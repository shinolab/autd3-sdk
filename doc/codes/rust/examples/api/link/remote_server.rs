use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs_link_echocat::EchocatLinkOption;
use autd3_rs_link_remote::RemoteServer;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let bind: SocketAddr = "0.0.0.0:8080".parse()?;
    let link = EchocatLinkOption::default();
    // ANCHOR: api
    RemoteServer::open(bind, link).await?.serve()?;
    // ANCHOR_END: api

    Ok(())
}
