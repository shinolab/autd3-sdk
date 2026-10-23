use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use autd3_cpu_wire::payload::{CpuConfigOutOfRange, PayloadBuildError};
use autd3_cpu_wire::value::FocusOutOfRange;
use autd3_rs_core::protocol::describe_device_error;

use crate::commands::PhaseDepth;
use crate::udp::UdpError;

#[derive(Clone)]
pub struct NetworkCause(Arc<UdpError>);

impl NetworkCause {
    #[must_use]
    pub(crate) fn new(source: UdpError) -> Self {
        Self(Arc::new(source))
    }
}

impl core::ops::Deref for NetworkCause {
    type Target = UdpError;

    fn deref(&self) -> &Self::Target {
        &self.0
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

    #[error("device {device} returned a shorter reply than the command defines")]
    UnexpectedReply { device: usize },

    #[error(
        "device {device} is lost: it stopped replying, so nothing is sent to it until the client is reopened"
    )]
    DeviceLost { device: usize },

    #[error("the devices did not reply within {timeout:?}")]
    Timeout { timeout: std::time::Duration },

    #[error("network error: {0}")]
    Network(#[source] NetworkCause),

    #[error("the device time is unknown: no reply carrying it has been observed yet")]
    DeviceTimeUnknown,

    #[error("invalid payload: {0}")]
    InvalidPayload(PayloadError),

    #[error("the client is closed")]
    Closed,
}

impl From<UdpError> for Error {
    fn from(e: UdpError) -> Self {
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
    #[error("the number of devices must be 1..={max}, got {got}")]
    DeviceCountOutOfRange { got: usize, max: usize },

    #[error("expected {expected} datagram(s) (one per device), got {got}")]
    DatagramCountMismatch { expected: usize, got: usize },

    #[error("datagram {index} has a payload of {len} bytes, more than {max}")]
    DatagramPayloadTooLong {
        index: usize,
        len: usize,
        max: usize,
    },

    #[error("the execution lead must be at least {min:?}, got {got:?}")]
    ExecLeadTooShort { got: Duration, min: Duration },

    #[error(
        "the execution time must be less than {limit:?} ahead of the device time, got {ahead:?}"
    )]
    ExecTooFarAhead { ahead: Duration, limit: Duration },

    #[error(
        "the execution time must be less than {limit:?} behind the device time, got {behind:?}"
    )]
    ExecTooFarBehind { behind: Duration, limit: Duration },

    #[error("modulation data must not be empty")]
    ModulationDataEmpty,

    #[error("modulation data of {len} samples exceeds the {max} that fit in one frame")]
    ModulationWriteTooLong { len: usize, max: usize },

    #[error("foci must not be empty")]
    FociEmpty,

    #[error("{len} control points exceed the {max} that fit in one frame")]
    FociWriteTooLong { len: usize, max: usize },

    #[error(
        "number of foci per sample must be 1..={}, got {num_foci}",
        crate::params::NUM_FOCI_MAX
    )]
    NumFociOutOfRange { num_foci: usize },

    #[error("sample {index} has {got} foci but sample 0 has {expected}")]
    NumFociMismatch {
        index: usize,
        expected: usize,
        got: usize,
    },

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

    #[error(transparent)]
    Build(#[from] PayloadBuildError),

    #[error(transparent)]
    CpuConfig(#[from] CpuConfigOutOfRange),

    #[error(transparent)]
    Focus(#[from] FocusOutOfRange),
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
