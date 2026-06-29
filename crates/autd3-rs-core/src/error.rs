use thiserror::Error;

use crate::mirror::SilencerAxis;
use crate::value::SamplingConfigError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("device {device} firmware error {code:#04x}: {}", crate::protocol::describe_device_error(*code))]
    DeviceError { device: usize, code: u8 },

    #[error(
        "device {device}: strict silencer {axis:?} completion {completion_steps} steps exceeds sampling divider {sampling_div}"
    )]
    SilencerConstraint {
        device: usize,
        axis: SilencerAxis,
        completion_steps: u16,
        sampling_div: u16,
    },

    #[error("ack timeout after {cycles} cycles")]
    Timeout { cycles: u32 },

    #[error("link error: {0}")]
    Link(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(PayloadError),

    #[error("client RT worker is no longer alive")]
    RtClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum PayloadError {
    #[error("max_inflight must be <= {max}")]
    MaxInFlightTooLarge { max: usize },

    #[error("link must expose 1..={max} devices, got {got}")]
    DeviceCountOutOfRange { got: usize, max: usize },

    #[error("geometry has {geometry} device(s) but link exposes {link}")]
    GeometryDeviceMismatch { geometry: usize, link: usize },

    #[error("expected {expected} datagram(s) (one per device), got {got}")]
    DatagramCountMismatch { expected: usize, got: usize },

    #[error("focus coordinate {axis} = {value} out of range {min}..={max}")]
    FocusOutOfRange {
        axis: &'static str,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error("modulation size {size} out of range 1..={max}")]
    ModulationSizeOutOfRange { size: usize, max: usize },

    #[error("modulation data must not be empty")]
    ModulationDataEmpty,

    #[error("modulation offset {offset} must be even (word-write-only RAM)")]
    ModulationOffsetNotEven { offset: usize },

    #[error("modulation write [{offset}, {end}) exceeds buffer capacity {capacity}")]
    ModulationWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },

    #[error("modulation sample count exceeds usize")]
    SampleCountOverflow,

    #[error("sine modulation value is out of range [0, 255]")]
    SineValueOutOfRange,

    #[error("square duty {duty} must be in range 0..=1")]
    DutyOutOfRange { duty: f32 },

    #[error("fourier components must not be empty")]
    FourierComponentsEmpty,

    #[error("all fourier components must have the same sampling config")]
    FourierSamplingConfigMismatch,

    #[error("fourier modulation value is out of range [0, 255]")]
    FourierValueOutOfRange,

    #[error("foci must not be empty")]
    FociEmpty,

    #[error("foci write [{offset}, {end}) exceeds capacity {capacity}")]
    FociWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },

    #[error("xor_hash data too large: max {max} bytes, got {len}")]
    XorHashDataTooLarge { len: usize, max: usize },

    #[error("silencer completion time {0:?} must be a multiple of the ultrasound period")]
    SilencerCompletionTimeNotMultiple(core::time::Duration),

    #[error("silencer completion time {0:?} is out of range (1..=65535 ultrasound periods)")]
    SilencerCompletionTimeOutOfRange(core::time::Duration),

    #[error("pattern size must be >= 1")]
    PatternSizeZero,

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

    #[error("STM size {size} out of range 1..={max}")]
    StmSizeOutOfRange { size: usize, max: usize },

    #[error("emissions has {len} entr(ies) but device {device} was requested")]
    EmissionsDeviceOutOfRange { device: usize, len: usize },

    #[error("pattern STM index {index} out of range 0..{max}")]
    PatternIndexOutOfRange { index: usize, max: usize },

    #[error("device {device} has no group key")]
    GroupKeyMissing { device: usize },

    #[error("device {device} maps to an unknown key")]
    GroupKeyUnknown { device: usize },

    #[error("frequency {hz} Hz is equal to or greater than the Nyquist frequency ({nyquist} Hz)")]
    FrequencyAboveNyquist { hz: f64, nyquist: f32 },

    #[error("modulation frequency must not be zero")]
    FrequencyZero,

    #[error("frequency {hz} Hz must be a valid positive value")]
    FrequencyNotPositive { hz: f32 },

    #[error("frequency {hz} Hz cannot be output with the current sampling config")]
    FrequencyNotRepresentable { hz: f32 },

    #[error("modulation frequency must be a valid value")]
    FrequencyNaN,

    #[error(transparent)]
    SamplingConfig(#[from] SamplingConfigError),
}
