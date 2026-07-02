use anyhow::Result;

use autd3_rs::commands::{PatternStm, PatternStmMode, PatternStmOption};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{LoopBehavior, PatternBank, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;
use autd3_rs_pattern::{FocusOption, focus, wavelength};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);
    let patterns = (0..200)
        .map(|i| {
            let theta = 2.0 * std::f32::consts::PI * i as f32 / 200.0;
            let target =
                center + offset(30.0 * theta.cos() * mm, 30.0 * theta.sin() * mm, 0.0 * mm);
            let mut buffer = geometry.pattern_buffer();
            focus(
                &geometry,
                target,
                wavelength,
                &FocusOption::default(),
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();

    let mut builder = client.datagram_builder();
    builder.push(PatternStm::new(
        1.0 * Hz,
        &patterns,
        PatternStmOption {
            bank: PatternBank::B0,
            mode: PatternStmMode::PhaseIntensityFull,
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
