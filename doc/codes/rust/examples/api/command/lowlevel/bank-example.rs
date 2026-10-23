use core::num::NonZeroU16;

use autd3_rs::commands::{
    ActivatePatternBank, Clear, ConfigPattern, Synchronize, WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;
use autd3_rs_pattern::{focus, wavelength};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;
    client.send((Clear, Synchronize)).await?;

    let mut phases = geometry.phase_buffer();
    focus(
        &geometry,
        geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
        wavelength(340.0 * m / s),
        &mut phases,
    );

    let bank = PatternBank::B0;

    client
        .send(WritePatternBuffer::new(bank, 0, &[phases], Intensity::MAX))
        .await?;
    client.send(ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size: 1,
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
