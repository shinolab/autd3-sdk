use core::num::NonZeroU16;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{Clear, FociStm, FociStmOption, Pattern, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{
    ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, SamplingConfig,
    TransitionMode,
};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let radius = (30.0 * mm).mm();
    let foci: Vec<ControlPoints> = (0..20)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 20.0;
            let p = center + Vector3::new(radius * theta.cos(), radius * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::from(p)], Intensity::MAX)
        })
        .collect();

    // ANCHOR: infinite
    // By default the playback loops infinitely; B0 keeps circling the focus.
    let config = SamplingConfig::from_freq(50.0 * Hz * foci.len() as f32)?;
    client.send(FociStm::new(config, &foci, FociStmOption::default())).await?;
    // ANCHOR_END: infinite

    // ANCHOR: finite
    // Play the circular motion only 3 times, then stop.
    // A finite loop (and non-immediate transition) only fires when switching to a
    // different bank, so write to bank B1 instead of the current B0.
    client.send(FociStm::new(
        config,
        &foci,
        FociStmOption {
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(3).unwrap()),
            bank: PatternBank::B1,
            transition_mode: TransitionMode::SyncIdx,
            ..Default::default()
        },
    )).await?;
    // ANCHOR_END: finite

    client.send(Pattern::new(&geometry.phase_buffer(), Intensity::MIN)).await?;
    client.close()?;
    Ok(())
}
