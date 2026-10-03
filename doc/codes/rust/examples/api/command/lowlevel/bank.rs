use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{
    ActivatePatternBank, ConfigPattern, PhaseDepth, WritePatternBuffer, WritePatternPhase,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs::udp::emulator::UdpEmulator;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let bank = PatternBank::B0;
    let index = 0;
    let _phases = geometry.phase_buffer();
    let phases = &_phases;
    let intensities = Intensity::MAX;
    // ANCHOR: write
    WritePatternBuffer::new(bank, index, phases, intensities);
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
    let index = 0;
    let depth = PhaseDepth::Bits4;
    let intensity = Intensity::MAX;
    // ANCHOR: compressed
    WritePatternPhase {
        bank,
        index,
        depth,
        intensity,
        patterns,
    };
    // ANCHOR_END: compressed
    Ok(())
}
