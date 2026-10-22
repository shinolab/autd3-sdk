use std::f32::consts::PI;

use autd3_rs::commands::{Clear, FociStm, FociStmOption, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{ControlPoints, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
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

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let points: Vec<ControlPoints> = (0..200)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 200.0;
            ControlPoints::from(center + offset(30.0 * mm * theta.cos(), 30.0 * mm * theta.sin(), 0.0 * mm))
        })
        .collect();

    client.send(FociStm::new(
        SamplingConfig::from_freq(1.0 * Hz * points.len() as f32)?,
        &points,
        FociStmOption {
            bank: PatternBank::B0,
            sound_speed: 340.0 * m / s,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
            ..Default::default()
        },
    )).await?;

    client.close()?;
    // HIDE
    Ok(())
}
// HIDE_END
