use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use crate::error::{Error, PayloadError};
use crate::protocol::MAX_INFLIGHT;

pub use crate::udp::bus::MAX_DEVICES;

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub ack_timeout: Duration,
    pub max_inflight: NonZeroUsize,
    pub max_resync_rounds: NonZeroU32,
    pub require_supported_firmware: bool,
    pub resynchronize_on_recovery: Option<NonZeroU32>,
    pub release_failsafe_on_recovery: Option<NonZeroU32>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            require_supported_firmware: false,
            resynchronize_on_recovery: NonZeroU32::new(3),
            release_failsafe_on_recovery: NonZeroU32::new(3),
        }
    }
}

impl ClientConfig {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_inflight.get() > MAX_INFLIGHT {
            return Err(PayloadError::MaxInflightTooLarge { max: MAX_INFLIGHT }.into());
        }
        if self.ack_timeout.is_zero() {
            return Err(PayloadError::ZeroAckTimeout.into());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_passes_validation() {
        assert!(ClientConfig::default().validate().is_ok());
    }

    #[test]
    fn max_inflight_above_the_device_queue_is_rejected() {
        let config = ClientConfig {
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT + 1).unwrap(),
            ..ClientConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(Error::InvalidPayload(PayloadError::MaxInflightTooLarge {
                max: MAX_INFLIGHT
            }))
        ));
    }
}
