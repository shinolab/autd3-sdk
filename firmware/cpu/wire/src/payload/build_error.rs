use core::time::Duration;

use super::PhaseDepth;

#[derive(Clone, Copy, PartialEq, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PayloadBuildError {
    #[error("modulation size {size} out of range {min}..={max}")]
    ModulationSizeOutOfRange { size: usize, min: usize, max: usize },
    #[error("modulation offset {offset} must be even (word-write-only RAM)")]
    ModulationOffsetNotEven { offset: usize },
    #[error("modulation write [{offset}, {end}) exceeds buffer capacity {capacity}")]
    ModulationWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },
    #[error("foci write [{offset}, {end}) exceeds capacity {capacity}")]
    FociWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },
    #[error("STM size {size} out of range {min}..={max}")]
    StmSizeOutOfRange { size: usize, min: usize, max: usize },
    #[error("a {size}-sample bank never advances its index, so it requires an infinite loop")]
    FiniteLoopNeedsMultipleSamples { size: usize },
    #[error("pattern size {size} must be >= {min}")]
    PatternSizeTooSmall { size: usize, min: usize },
    #[error("num_foci {num_foci} out of range 1..={max}")]
    NumFociOutOfRange { num_foci: u8, max: u8 },
    #[error("STM size {size} x num_foci {num_foci} exceeds capacity {capacity}")]
    StmFociExceedCapacity {
        size: usize,
        num_foci: u8,
        capacity: usize,
    },
    #[error("sound_speed must be >= 1")]
    SoundSpeedZero,
    #[error("sound_speed {m_s} m/s exceeds the representable maximum {max} m/s")]
    SoundSpeedTooLarge { m_s: f32, max: f32 },
    #[error("silencer completion time {0:?} must be a multiple of the ultrasound period")]
    SilencerCompletionTimeNotMultiple(Duration),
    #[error("silencer completion time {0:?} is out of range (1..=65535 ultrasound periods)")]
    SilencerCompletionTimeOutOfRange(Duration),
    #[error("{count} patterns do not fit the {depth:?} phase depth, which carries {max} per frame")]
    PatternCountExceedsDepth {
        count: usize,
        depth: PhaseDepth,
        max: usize,
    },
    #[error("{count} patterns do not fit one frame, which carries {max}")]
    PatternCountExceedsFrame { count: usize, max: usize },
    #[error("pattern STM index {index} out of range 0..{max}")]
    PatternIndexOutOfRange { index: usize, max: usize },
}
