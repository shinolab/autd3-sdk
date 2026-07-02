use anyhow::Result;

use autd3_rs::commands::{
    ChangePatternBank, ConfigPattern, PatternCompression, StmConfig, WritePatternCompressed,
};
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

    let wavelength = wavelength(340.0 * m / s);
    let patterns = [-30.0f32, -10.0, 10.0, 30.0]
        .iter()
        .map(|&x| {
            let mut buffer = geometry.pattern_buffer();
            focus(
                &geometry,
                geometry.center() + offset(x * mm, 0.0 * mm, 150.0 * mm),
                wavelength,
                &FocusOption::default(),
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();

    let bank = PatternBank::B0;

    let mut builder = client.datagram_builder();
    builder.push(WritePatternCompressed {
        bank,
        index: 0,
        format: PatternCompression::PhaseHalf,
        patterns: [
            Some(&patterns[0][..]),
            Some(&patterns[1][..]),
            Some(&patterns[2][..]),
            Some(&patterns[3][..]),
        ],
    });
    builder.push(ConfigPattern {
        bank,
        config: StmConfig::new(1.0 * Hz).into_sampling_config(patterns.len()),
        size: patterns.len(),
        loop_behavior: LoopBehavior::Infinite,
    });
    builder.push(ChangePatternBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    });
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    Ok(())
}
