use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, SERVER_PORT, ServerLink};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let port = SERVER_PORT;
    let timeout = Duration::from_secs(1);
    // ANCHOR: api
    let link = ServerLink { port, timeout };
    let client = Client::open(&geometry, &link, ClientConfig::default()).await?;
    // ANCHOR_END: api

    Ok(())
}
