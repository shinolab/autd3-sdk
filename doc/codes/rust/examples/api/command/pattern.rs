use core::num::NonZeroU16;

use autd3_rs::DatagramBuilder;
use autd3_rs::commands::{ChangePatternBank, ConfigPattern, Pattern, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{LoopBehavior, PatternBank, SamplingConfig, TransitionMode};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let bank = PatternBank::B0;

    let emissions = geometry.pattern_buffer();

    // ANCHOR: api
    Pattern::new(&emissions);

    Pattern::with_bank(bank, &emissions);

    Pattern {
        bank,
        emissions: &emissions,
    };
    // ANCHOR_END: api

    let emissions = &emissions;
    // ANCHOR: equivalent
    WritePatternBuffer {
        bank,
        index: 0,
        emissions,
    };
    ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    };
    ChangePatternBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    };
    // ANCHOR_END: equivalent
}
