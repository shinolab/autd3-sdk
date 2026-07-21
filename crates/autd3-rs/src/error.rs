use thiserror::Error;

use autd3_rs_core::error::{EncodeError, LinkError};
use autd3_rs_core::mirror::{BankLoop, SilencerAxis};
use autd3_rs_core::protocol::describe_device_error;
use autd3_rs_core::value::{PulseWidthError, SamplingConfigError, TransitionMode};

#[derive(Debug, Error)]
pub enum Error {
    #[error("device {device} firmware error {code:#04x}: {}", describe_device_error(*code))]
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

    #[error(
        "device {device}: transition mode {transition_mode:?} is invalid for a {bank_loop:?} loop bank"
    )]
    TransitionConstraint {
        device: usize,
        transition_mode: TransitionMode,
        bank_loop: BankLoop,
    },

    #[error("ack timeout after {cycles} cycles")]
    Timeout { cycles: u32 },

    #[error("link error: {0}")]
    Link(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(PayloadError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error("client RT worker is no longer alive")]
    RtClosed,
}

impl From<LinkError> for Error {
    fn from(e: LinkError) -> Self {
        Error::Link(e.0)
    }
}

impl<E> From<E> for Error
where
    E: Into<PayloadError>,
{
    fn from(value: E) -> Self {
        Error::InvalidPayload(value.into())
    }
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

    #[error("foci must not be empty")]
    FociEmpty,

    #[error("foci write [{offset}, {end}) exceeds capacity {capacity}")]
    FociWriteExceedsCapacity {
        offset: usize,
        end: usize,
        capacity: usize,
    },

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

    #[error("device {device} has {got} transducer entr(ies) but {expected} are required")]
    TransducerCountMismatch {
        device: usize,
        got: usize,
        expected: usize,
    },

    #[error("pattern STM index {index} out of range 0..{max}")]
    PatternIndexOutOfRange { index: usize, max: usize },

    #[error(transparent)]
    SamplingConfig(#[from] SamplingConfigError),

    #[error(transparent)]
    PulseWidth(#[from] PulseWidthError),
}
