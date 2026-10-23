use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{
    ActivatePatternBank, ConfigPattern, PhaseDepth, WritePatternBuffer, WritePatternPhase,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let bank = PatternBank::B0;
    let start_index = 0;
    let _patterns = [geometry.phase_buffer()];
    let patterns = &_patterns;
    let intensities = Intensity::MAX;
    // ANCHOR: write
    WritePatternBuffer::new(bank, start_index, patterns, intensities);
    // ANCHOR_END: write
    let config = SamplingConfig::FREQ_4K;
    let size = 1;
    let loop_behavior = LoopBehavior::Infinite;
    // ANCHOR: config
    ConfigPattern {
        bank,
        config,
        size,
        loop_behavior,
    };
    // ANCHOR_END: config
    let transition_mode = TransitionMode::Immediate;
    // ANCHOR: change
    ActivatePatternBank {
        bank,
        transition_mode,
    };
    // ANCHOR_END: change

    let _patterns = vec![geometry.phase_buffer(); 4];
    let patterns = &_patterns;
    let start_index = 0;
    let depth = PhaseDepth::Bits4;
    let intensity = Intensity::MAX;
    // ANCHOR: compressed
    WritePatternPhase {
        bank,
        start_index,
        depth,
        intensity,
        patterns,
    };
    // ANCHOR_END: compressed
    Ok(())
}
