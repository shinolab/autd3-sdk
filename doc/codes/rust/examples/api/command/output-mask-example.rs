use autd3_rs::commands::SetOutputMask;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::UdpOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = UdpOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let masks = geometry.output_mask_buffer();

    client.send(SetOutputMask { masks: &masks }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
