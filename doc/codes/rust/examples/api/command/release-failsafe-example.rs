use autd3_rs::commands::{Clear, ReleaseFailsafe, Synchronize};
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

    let status = client.read_state().await?.device_status();
    if status.iter().any(|s| s.failsafe_active) {
        client.send(ReleaseFailsafe).await?;
    }

    // HIDE
    Ok(())
}
// HIDE_END
