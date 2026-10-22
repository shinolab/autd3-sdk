use autd3_rs::commands::{Clear, EmulateGpioIn, Synchronize};
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

    client.send(EmulateGpioIn {
        values: [true, false, false, false],
    }).await?;

    client.close()?;
    // HIDE
    Ok(())
}
// HIDE_END
