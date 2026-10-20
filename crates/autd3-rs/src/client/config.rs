use std::num::NonZeroUsize;
use std::time::Duration;

use crate::error::{ClientConfigError, Error};
use crate::protocol::MAX_INFLIGHT;

pub use crate::udp::bus::MAX_DEVICES;

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub ack_timeout: Duration,
    pub max_inflight: NonZeroUsize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT).unwrap(),
        }
    }
}

impl ClientConfig {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_inflight.get() > MAX_INFLIGHT {
            return Err(Error::InvalidConfig(
                ClientConfigError::MaxInflightTooLarge { max: MAX_INFLIGHT },
            ));
        }
        if self.ack_timeout.is_zero() {
            return Err(Error::InvalidConfig(ClientConfigError::ZeroAckTimeout));
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
            Err(Error::InvalidConfig(
                ClientConfigError::MaxInflightTooLarge { max: MAX_INFLIGHT }
            ))
        ));
    }

    #[test]
    fn a_zero_ack_timeout_is_rejected() {
        let config = ClientConfig {
            ack_timeout: Duration::ZERO,
            ..ClientConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(Error::InvalidConfig(ClientConfigError::ZeroAckTimeout))
        ));
    }
}
