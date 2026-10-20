use std::time::Duration;
use autd3_rs::commands::ForceFan;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    client.send(ForceFan { value: true }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
