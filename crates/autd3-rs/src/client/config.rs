use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use crate::error::{Error, PayloadError};
use crate::protocol::MAX_INFLIGHT;
use autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES;

pub use crate::udp::bus::MAX_DEVICES;

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub ack_timeout: Duration,
    pub max_inflight: NonZeroUsize,
    pub max_resync_rounds: NonZeroU32,
    pub require_supported_firmware: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(DEVICE_QUEUE_FRAMES).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            require_supported_firmware: false,
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
