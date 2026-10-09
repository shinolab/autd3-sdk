use core::time::Duration;

use super::PhaseDepth;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PayloadBuildError {
    ModulationSizeOutOfRange {
        size: usize,
        min: usize,
        max: usize,
    },
    ModulationOffsetNotEven {
        offset: usize,
    },
    ModulationWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },
    FociWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },
    StmSizeOutOfRange {
        size: usize,
        min: usize,
        max: usize,
    },
    FiniteLoopNeedsMultipleSamples {
        size: usize,
    },
    PatternSizeTooSmall {
        size: usize,
        min: usize,
    },
    NumFociOutOfRange {
        num_foci: u8,
        max: u8,
    },
    StmFociExceedCapacity {
        size: usize,
        num_foci: u8,
        capacity: usize,
    },
    SoundSpeedZero,
    SoundSpeedTooLarge {
        m_s: f32,
        max: f32,
    },
    SilencerCompletionTimeNotMultiple(Duration),
    SilencerCompletionTimeOutOfRange(Duration),
    PatternCountExceedsDepth {
        count: usize,
        depth: PhaseDepth,
        max: usize,
    },
    PatternCountExceedsFrame {
        count: usize,
        max: usize,
    },
    PatternIndexOutOfRange {
        index: usize,
        max: usize,
    },
}
