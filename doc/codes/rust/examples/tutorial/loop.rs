use core::num::NonZeroU16;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{FociStm, FociStmOption};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{
    ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, TransitionMode,
};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let radius = (30.0 * mm).mm();
    let foci: Vec<ControlPoints<1>> = (0..20)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 20.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();

    // ANCHOR: infinite
    // By default the playback loops infinitely; B0 keeps circling the focus.
    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(50.0 * Hz, &foci, FociStmOption::default()));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: infinite

    // ANCHOR: finite
    // Play the circular motion only 3 times, then stop.
    // A finite loop (and non-immediate transition) only fires when switching to a
    // different bank, so write to bank B1 instead of the current B0.
    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(
        50.0 * Hz,
        &foci,
        FociStmOption {
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(3).unwrap()),
            bank: PatternBank::B1,
            transition_mode: TransitionMode::SyncIdx,
            ..Default::default()
        },
    ));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: finite

    client.close().await?;
    Ok(())
}
