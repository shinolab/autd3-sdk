use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{ChangePatternBank, ConfigPattern, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;
use autd3_rs_pattern::{FocusOption, focus, wavelength};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let mut emissions = geometry.pattern_buffer();
    focus(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        wavelength(340.0 * m / s),
        &FocusOption::default(),
        &mut emissions,
    );

    let bank = PatternBank::B0;

    let mut builder = client.datagram_builder();
    builder.push(WritePatternBuffer {
        bank,
        index: 0,
        emissions: &emissions,
    });
    builder.push(ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size: 1,
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
