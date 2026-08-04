use core::time::Duration;

use autd3_rs_core::error::{EncodeError, LinkError};
use autd3_rs_core::value::{PulseWidthError, SamplingConfigError};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeoutPhase {
    Handshake,
    Command { tag: u8 },
}

impl core::fmt::Display for TimeoutPhase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TimeoutPhase::Handshake => write!(f, "handshake"),
            TimeoutPhase::Command { tag } => write!(f, "command {tag:#04x}"),
        }
    }
}

pub(crate) const NO_ERROR: u8 = 0x00;
pub const NOT_SUPPORTED_TAG: u8 = 0x01;
pub const INVALID_MSG_ID: u8 = 0x02;
pub const INVALID_INFO_TYPE: u8 = 0x03;
pub const INVALID_GAIN_STM_MODE: u8 = 0x04;
pub const INVALID_SEGMENT_TRANSITION: u8 = 0x05;
pub const MISS_TRANSITION_TIME: u8 = 0x06;
pub const INVALID_SILENCER_SETTINGS: u8 = 0x07;
pub const INVALID_TRANSITION_MODE: u8 = 0x08;

#[must_use]
pub const fn describe_device_error(code: u8) -> &'static str {
    match code {
        NO_ERROR => "no error",
        NOT_SUPPORTED_TAG => "the device does not support the sent tag",
        INVALID_MSG_ID => "the message id is out of range",
        INVALID_INFO_TYPE => "the firmware info type is out of range",
        INVALID_GAIN_STM_MODE => "the GainSTM mode is out of range",
        INVALID_SEGMENT_TRANSITION => "the requested segment cannot be activated",
        MISS_TRANSITION_TIME => "the requested transition time has already passed",
        INVALID_SILENCER_SETTINGS => "the silencer completion time exceeds the sampling period",
        INVALID_TRANSITION_MODE => "the transition mode is invalid for the target segment",
        _ => "unknown firmware error",
    }
}

#[derive(Debug, Error)]
pub enum LegacyError {
    #[error("device {device} firmware error {code:#04x}: {}", describe_device_error(*code))]
    Device { device: usize, code: u8 },

    #[error(
        "{phase} timed out after {cycles} cycles waiting for msg_id {expected:#04x} \
         (device acks: {acks}; {stale_cycles} of those cycles were stale)"
    )]
    Timeout {
        phase: TimeoutPhase,
        cycles: u32,
        expected: u8,
        acks: String,
        stale_cycles: u32,
    },

    #[error(
        "{phase}: the bus never completed a cycle in {cycles} tries — every cycle was stale, \
         so the devices are not all in OP (check the EtherCAT state and DC configuration; \
         a device whose firmware is wedged stops answering and drops out of OP)"
    )]
    BusNotOperational { phase: TimeoutPhase, cycles: u32 },

    #[error("link error: {0}")]
    Link(String),

    #[error(
        "device {device} reports CPU firmware {version}, but this client only supports legacy-v12.1.0"
    )]
    UnsupportedFirmware { device: usize, version: String },

    #[error(
        "device {device} returned fpga state {state:#04x} with the valid bit clear; \
         the fpga state reads were turned off (a Clear or ReadsFpgaState(false) command) \
         and re-enabling them did not take effect"
    )]
    FpgaStateInvalid { device: usize, state: u8 },

    #[error("geometry has {geometry} device(s) but the link exposes {link}")]
    DeviceCountMismatch { geometry: usize, link: usize },

    #[error("the link must expose at least one device")]
    NoDevices,

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    SamplingConfig(#[from] SamplingConfigError),

    #[error(transparent)]
    PulseWidth(#[from] PulseWidthError),

    #[error("invalid payload: {0}")]
    InvalidPayload(#[from] PayloadError),

    #[error("legacy RT worker is no longer alive")]
    RtClosed,

    #[error("the legacy client is closing or already closed")]
    Closed,
}

impl From<LinkError> for LegacyError {
    fn from(e: LinkError) -> Self {
        LegacyError::Link(e.message().to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum PayloadError {
    #[error("the link must expose 1..={max} devices, got {got}")]
    DeviceCountOutOfRange { got: usize, max: usize },

    #[error("expected a frame for {expected} device(s), got one for {got}")]
    FrameDeviceCountMismatch { expected: usize, got: usize },

    #[error("modulation size {size} out of range {min}..={max}")]
    ModulationSizeOutOfRange { size: usize, min: usize, max: usize },

    #[error("FociSTM num_foci {num_foci} out of range {min}..={max}")]
    NumFociOutOfRange {
        num_foci: usize,
        min: usize,
        max: usize,
    },

    #[error("FociSTM total foci {total} out of range {min}..={max}")]
    FociStmTotalSizeOutOfRange {
        total: usize,
        min: usize,
        max: usize,
    },

    #[error("GainSTM size {size} out of range {min}..={max}")]
    GainStmSizeOutOfRange { size: usize, min: usize, max: usize },

    #[error("emission buffer has {got} slot(s) but the geometry has {expected} device(s)")]
    EmissionDeviceCountMismatch { expected: usize, got: usize },

    #[error("device {device} has {expected} transducer(s) but the buffer holds {got}")]
    EmissionTransducerCountMismatch {
        device: usize,
        expected: usize,
        got: usize,
    },

    #[error("silencer completion time {0:?} must be a multiple of the ultrasound period")]
    SilencerCompletionTimeNotMultiple(Duration),

    #[error("silencer completion time {0:?} is out of range (1..=65535 ultrasound periods)")]
    SilencerCompletionTimeOutOfRange(Duration),
}

pub(crate) fn check_device_error(device: usize, code: u8) -> Result<(), LegacyError> {
    if code == NO_ERROR {
        Ok(())
    } else {
        Err(LegacyError::Device { device, code })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_legacy_firmware() {
        assert_eq!(NOT_SUPPORTED_TAG, 0x01);
        assert_eq!(INVALID_MSG_ID, 0x02);
        assert_eq!(INVALID_INFO_TYPE, 0x03);
        assert_eq!(INVALID_GAIN_STM_MODE, 0x04);
        assert_eq!(INVALID_SEGMENT_TRANSITION, 0x05);
        assert_eq!(MISS_TRANSITION_TIME, 0x06);
        assert_eq!(INVALID_SILENCER_SETTINGS, 0x07);
        assert_eq!(INVALID_TRANSITION_MODE, 0x08);
    }

    #[test]
    fn unknown_codes_get_a_generic_description() {
        assert_eq!(describe_device_error(0x0F), "unknown firmware error");
    }

    #[test]
    fn check_device_error_passes_only_zero() {
        assert!(check_device_error(0, NO_ERROR).is_ok());
        let e = check_device_error(3, INVALID_MSG_ID).unwrap_err();
        assert!(matches!(
            e,
            LegacyError::Device {
                device: 3,
                code: INVALID_MSG_ID
            }
        ));
        assert_eq!(
            e.to_string(),
            "device 3 firmware error 0x02: the message id is out of range"
        );
    }
}
