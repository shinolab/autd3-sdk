use anyhow::Result;

use autd3_rs::commands::{FociStm, FociStmOption, Modulation, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig, ControlPoints};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);

    // ANCHOR: stm
    let points = [
        ControlPoints::from(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm)),
        ControlPoints::from(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm)),
    ];
    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(0.5 * Hz, &points, FociStmOption::default()));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: stm

    client.close().await?;
    Ok(())
}
