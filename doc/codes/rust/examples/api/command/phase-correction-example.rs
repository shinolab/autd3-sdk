use autd3_rs::commands::{Clear, SetPhaseCorrection, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send((Clear, Synchronize)).await?;

    let phases = geometry.phase_buffer();

    client.send(SetPhaseCorrection { phases }).await?;

    client.close()?;
    // HIDE
    Ok(())
}
// HIDE_END
