use core::num::NonZeroU16;

use autd3_rs::DatagramBuilder;
use autd3_rs::commands::{ChangePatternBank, ConfigPattern, Pattern, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let bank = PatternBank::B0;
    let transition_mode = TransitionMode::Immediate;

    let phases = geometry.phase_buffer();
    let intensities = Intensity::MAX;

    // ANCHOR: api
    Pattern::new(&phases, intensities);

    Pattern::with_bank(bank, &phases, intensities);

    Pattern {
        bank,
        phases: &phases,
        intensities: intensities.into(),
        transition_mode,
    };
    // ANCHOR_END: api

    let phases = &phases;
    // ANCHOR: equivalent
    WritePatternBuffer::new(bank, 0, phases, intensities);
    ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    };
    ChangePatternBank {
        bank,
        transition_mode,
    };
    // ANCHOR_END: equivalent
}
