use anyhow::Result;

use autd3_rs::commands::{Clear, FociStm, FociStmOption, Modulation, Pattern, SetSilencer, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{ControlPoints, Intensity, SamplingConfig};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send((Clear, Synchronize)).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);

    // ANCHOR: stm
    let points = [
        ControlPoints::from(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm)),
        ControlPoints::from(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm)),
    ];
    let config = SamplingConfig::from_freq(0.5 * Hz * points.len() as f32)?;
    client.send(FociStm::new(config, &points, FociStmOption::default())).await?;
    // ANCHOR_END: stm

    client.send(Pattern::new(&geometry.phase_buffer(), Intensity::MIN)).await?;
    client.close()?;
    Ok(())
}
