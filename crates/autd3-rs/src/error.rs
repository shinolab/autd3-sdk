use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use autd3_cpu_wire::payload::PayloadBuildError;
use autd3_rs_core::error::EncodeError;
use autd3_rs_core::protocol::describe_device_error;

use crate::commands::PhaseDepth;
use crate::firmware_version::FirmwareVersion;
use autd3_rs_core::value::{PulseWidthError, SamplingConfigError};

#[derive(Clone)]
pub struct NetworkCause(Arc<dyn core::error::Error + Send + Sync>);

impl NetworkCause {
    #[must_use]
    pub fn new<E: core::error::Error + Send + Sync + 'static>(source: E) -> Self {
        Self(Arc::new(source))
    }
}

impl core::ops::Deref for NetworkCause {
    type Target = dyn core::error::Error + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl core::fmt::Debug for NetworkCause {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&*self.0, f)
    }
}

impl core::fmt::Display for NetworkCause {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&*self.0, f)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("device {device} firmware error {code:#04x}: {}", describe_device_error(*code))]
    DeviceError { device: usize, code: u8 },

    #[error(
        "device {device} runs firmware {version}, which is outside the series supported by this SDK ({}.{}.x)",
        FirmwareVersion::SUPPORTED_SERIES.0,
        FirmwareVersion::SUPPORTED_SERIES.1
    )]
    UnsupportedFirmware {
        device: usize,
        version: FirmwareVersion,
    },

    #[error("device {device} returned a shorter reply than the command defines")]
    UnexpectedReply { device: usize },

    #[error(
        "device {device} is lost: it stopped replying, so nothing is sent to it until the client is reopened"
    )]
    DeviceLost { device: usize },

    #[error("the devices did not reply within {timeout:?}")]
    Timeout { timeout: std::time::Duration },

    #[error(
        "device {device} did not accept the frame: it acknowledged sequence {got} where {expected} was sent"
    )]
    SeqMismatch {
        device: usize,
        expected: u8,
        got: u8,
    },

    #[error("network error: {0}")]
    Network(#[source] NetworkCause),

    #[error("the device time is unknown: no reply carrying it has been observed yet")]
    DeviceTimeUnknown,

    #[error("invalid payload: {0}")]
    InvalidPayload(PayloadError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error("the client is closed")]
    Closed,
}

impl From<crate::udp::UdpError> for Error {
    fn from(e: crate::udp::UdpError) -> Self {
        Error::Network(NetworkCause::new(e))
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
#[non_exhaustive]
pub enum PayloadError {
    #[error("max_inflight must be <= {max}")]
    MaxInflightTooLarge { max: usize },

    #[error("ack_timeout must be longer than zero")]
    ZeroAckTimeout,

    #[error("the number of devices must be 1..={max}, got {got}")]
    DeviceCountOutOfRange { got: usize, max: usize },

    #[error("expected {expected} datagram(s) (one per device), got {got}")]
    DatagramCountMismatch { expected: usize, got: usize },

    #[error(
        "CPU config `{field}` = {value:?} must be a multiple of {unit:?} within {min:?}..={max:?}"
    )]
    CpuConfigOutOfRange {
        field: &'static str,
        value: Duration,
        unit: Duration,
        min: Duration,
        max: Duration,
    },

    #[error("modulation size {size} out of range {min}..={max}")]
    ModulationSizeOutOfRange { size: usize, min: usize, max: usize },

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

    #[error("pattern size {size} must be >= {min}")]
    PatternSizeTooSmall { size: usize, min: usize },

    #[error("{count} patterns do not fit the {depth:?} phase depth, which carries {max} per frame")]
    PatternCountExceedsDepth {
        count: usize,
        depth: PhaseDepth,
        max: usize,
    },

    #[error("a {size}-sample bank never advances its index, so it requires an infinite loop")]
    FiniteLoopNeedsMultipleSamples { size: usize },

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

    #[error("STM size {size} out of range {min}..={max}")]
    StmSizeOutOfRange { size: usize, min: usize, max: usize },

    #[error("per-device data has {len} entr(ies) but device {device} was requested")]
    DeviceDataOutOfRange { device: usize, len: usize },

    #[error("device {device} has {got} transducer entr(ies) but {expected} are required")]
    TransducerCountMismatch {
        device: usize,
        got: usize,
        expected: usize,
    },

    #[error("pattern STM has {phases} phase pattern(s) but {intensities} intensity pattern(s)")]
    PatternStmLengthMismatch { phases: usize, intensities: usize },

    #[error(
        "the {depth:?} phase depth carries a single uniform intensity; per-transducer or per-index intensities are not supported"
    )]
    PhaseDepthRequiresUniformIntensity { depth: PhaseDepth },

    #[error("pattern STM index {index} out of range 0..{max}")]
    PatternIndexOutOfRange { index: usize, max: usize },

    #[error("{count} patterns do not fit one frame, which carries {max}")]
    PatternCountExceedsFrame { count: usize, max: usize },

    #[error(transparent)]
    SamplingConfig(#[from] SamplingConfigError),

    #[error(transparent)]
    PulseWidth(#[from] PulseWidthError),
}

impl From<PayloadBuildError> for PayloadError {
    fn from(e: PayloadBuildError) -> Self {
        match e {
            PayloadBuildError::ModulationSizeOutOfRange { size, min, max } => {
                Self::ModulationSizeOutOfRange { size, min, max }
            }
            PayloadBuildError::ModulationOffsetNotEven { offset } => {
                Self::ModulationOffsetNotEven { offset }
            }
            PayloadBuildError::ModulationWriteExceedsCapacity {
                offset,
                end,
                capacity,
            } => Self::ModulationWriteExceedsCapacity {
                offset,
                end,
                capacity,
            },
            PayloadBuildError::FociWriteExceedsCapacity {
                offset,
                end,
                capacity,
            } => Self::FociWriteExceedsCapacity {
                offset,
                end,
                capacity,
            },
            PayloadBuildError::StmSizeOutOfRange { size, min, max } => {
                Self::StmSizeOutOfRange { size, min, max }
            }
            PayloadBuildError::FiniteLoopNeedsMultipleSamples { size } => {
                Self::FiniteLoopNeedsMultipleSamples { size }
            }
            PayloadBuildError::PatternSizeTooSmall { size, min } => {
                Self::PatternSizeTooSmall { size, min }
            }
            PayloadBuildError::NumFociOutOfRange { num_foci, max } => {
                Self::NumFociOutOfRange { num_foci, max }
            }
            PayloadBuildError::StmFociExceedCapacity {
                size,
                num_foci,
                capacity,
            } => Self::StmFociExceedCapacity {
                size,
                num_foci,
                capacity,
            },
            PayloadBuildError::SoundSpeedZero => Self::SoundSpeedZero,
            PayloadBuildError::SoundSpeedTooLarge { m_s, max } => {
                Self::SoundSpeedTooLarge { m_s, max }
            }
            PayloadBuildError::SilencerCompletionTimeNotMultiple(time) => {
                Self::SilencerCompletionTimeNotMultiple(time)
            }
            PayloadBuildError::SilencerCompletionTimeOutOfRange(time) => {
                Self::SilencerCompletionTimeOutOfRange(time)
            }
            PayloadBuildError::PatternCountExceedsDepth { count, depth, max } => {
                Self::PatternCountExceedsDepth { count, depth, max }
            }
            PayloadBuildError::PatternCountExceedsFrame { count, max } => {
                Self::PatternCountExceedsFrame { count, max }
            }
            PayloadBuildError::PatternIndexOutOfRange { index, max } => {
                Self::PatternIndexOutOfRange { index, max }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(e: &Error) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur: Option<&(dyn core::error::Error + 'static)> = core::error::Error::source(e);
        while let Some(e) = cur {
            out.push(e.to_string());
            cur = e.source();
        }
        out
    }

    #[test]
    fn a_network_error_keeps_its_source_when_it_becomes_a_client_error() {
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let message = io.to_string();
        let e = Error::from(crate::udp::UdpError::from(io));

        assert_eq!(e.to_string(), format!("network error: {message}"));
        assert_eq!(chain(&e), vec![message.clone(), message]);

        let udp_error = core::error::Error::source(&e)
            .expect("the cause must be reachable through source()")
            .downcast_ref::<crate::udp::UdpError>()
            .expect("the UdpError itself must survive the conversion");
        assert_eq!(
            core::error::Error::source(udp_error)
                .expect("the source must survive")
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn a_network_error_without_a_source_ends_the_chain() {
        let e = Error::from(crate::udp::UdpError::Closed);

        assert_eq!(e.to_string(), "network error: the connection is closed");
        assert_eq!(chain(&e), vec!["the connection is closed".to_owned()]);
    }
}
