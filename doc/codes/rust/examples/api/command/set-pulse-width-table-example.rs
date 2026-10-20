use autd3_rs::commands::{Clear, SetPulseWidthTable, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;
use autd3_rs::value::PulseWidth;

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option).await?;
    client.send((Clear, Synchronize)).await?;

    let mut table = SetPulseWidthTable::empty_table();
    for (width, i) in table.iter_mut().zip(0..) {
        *width = PulseWidth::new(i);
    }

    client.send(SetPulseWidthTable { table: &table }).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
