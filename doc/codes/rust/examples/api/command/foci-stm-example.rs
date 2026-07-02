use anyhow::Result;

use autd3_rs::commands::{FociStm, FociStmOption, circle};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut points = Vec::new();
    circle(center, 30.0 * mm, 200, Vector3::z_axis(), Intensity::MAX, &mut points);

    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(
        1.0 * Hz,
        &points,
        FociStmOption {
            bank: PatternBank::B0,
            sound_speed: 340.0 * m / s,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        },
    ));
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    Ok(())
}
