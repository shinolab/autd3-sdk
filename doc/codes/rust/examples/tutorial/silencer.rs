use std::f32::consts::PI;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{
    Clear, FixedCompletionTime, FociStm, FociStmOption, Pattern, SetSilencer, Synchronize,
};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{ControlPoint, ControlPoints, Intensity, SamplingConfig};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send((Clear, Synchronize)).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let radius = (30.0 * mm).mm();

    // ANCHOR: disable
    let foci: Vec<ControlPoints<1>> = (0..20)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 20.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();
    client.send(SetSilencer::disable()).await?;
    client.send(FociStm::new(
        SamplingConfig::from_freq(50.0 * Hz * foci.len() as f32)?,
        &foci,
        FociStmOption::default(),
    )).await?;
    // ANCHOR_END: disable

    // ANCHOR: err
    let foci: Vec<ControlPoints<1>> = (0..40)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 40.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();
    client.send(SetSilencer::default()).await?;
    client.send(FociStm::new(
        SamplingConfig::from_freq(50.0 * Hz * foci.len() as f32)?,
        &foci,
        FociStmOption::default(),
    )).await?;
    // ANCHOR_END: err

    // ANCHOR: workaround
    let foci: Vec<ControlPoints<1>> = (0..40)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 40.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();
    client.send(SetSilencer::new(FixedCompletionTime {
        intensity: Duration::from_micros(500),
        phase: Duration::from_micros(500),
        strict_mode: true,
    })).await?;
    let config = SamplingConfig::from_freq(50.0 * Hz * foci.len() as f32)?;
    client.send(FociStm::new(config, &foci, FociStmOption::default())).await?;
    // ANCHOR_END: workaround

    client.send(Pattern::new(&geometry.phase_buffer(), Intensity::MIN)).await?;
    client.close()?;
    Ok(())
}
