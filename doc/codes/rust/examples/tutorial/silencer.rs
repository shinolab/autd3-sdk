use std::f32::consts::PI;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{FixedCompletionTime, FociStm, FociStmOption, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{ControlPoint, ControlPoints, Intensity};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

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
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::disable()).push(FociStm::new(
        50.0 * Hz,
        &foci,
        FociStmOption::default(),
    ));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: disable

    // ANCHOR: err
    let foci: Vec<ControlPoints<1>> = (0..40)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 40.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push(FociStm::new(
        50.0 * Hz,
        &foci,
        FociStmOption::default(),
    ));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: err

    // ANCHOR: workaround
    let foci: Vec<ControlPoints<1>> = (0..40)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 40.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();
    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::new(FixedCompletionTime {
            intensity: Duration::from_micros(500),
            phase: Duration::from_micros(500),
            strict_mode: true,
        }))
        .push(FociStm::new(50.0 * Hz, &foci, FociStmOption::default()));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: workaround

    client.close().await?;
    Ok(())
}
