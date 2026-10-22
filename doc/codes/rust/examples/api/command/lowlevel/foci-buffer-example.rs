use std::f32::consts::PI;

use autd3_rs::commands::{
    ActivatePatternBank, Clear, ConfigFociStm, Synchronize, WriteFociBuffer,
};
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
    let points: Vec<ControlPoints<1>> = (0..200)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 200.0;
            ControlPoints::from(center + offset(30.0 * mm * theta.cos(), 30.0 * mm * theta.sin(), 0.0 * mm))
        })
        .collect();

    let bank = PatternBank::B0;

    let max_len = WriteFociBuffer::<1>::MAX_LEN;
    for (i, chunk) in points.chunks(max_len).enumerate() {
        client.send(WriteFociBuffer {
            bank,
            start_index: i * max_len,
            points: chunk,
        }).await?;
    }
    client.send(ConfigFociStm {
        bank,
        config: SamplingConfig::from_freq(1.0 * Hz * points.len() as f32)?,
        size: points.len(),
        num_foci: 1,
        sound_speed: 340.0 * m / s,
        loop_behavior: LoopBehavior::Infinite,
    }).await?;
    client.send(ActivatePatternBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    }).await?;

    client.close()?;
    // HIDE
    Ok(())
}
// HIDE_END
