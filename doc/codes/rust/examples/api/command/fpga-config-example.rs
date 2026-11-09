use autd3_rs::commands::{Clear, FpgaConfig, SetFpgaConfig, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let mut client = Client::open(&geometry, &option).await?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;

    client.send(SetFpgaConfig::new(FpgaConfig {
        sync_phase_lock: false,
        ..Default::default()
    })).await?;

    client.send(SetFpgaConfig::default()).await?;

    // HIDE
    Ok(())
}
// HIDE_END
