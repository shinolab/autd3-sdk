use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use crate::driver::TransportConfig;
use crate::error::{Error, PayloadError};
use crate::protocol::MAX_INFLIGHT;
use autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES;

pub const MAX_DEVICES: usize = 128;
pub const ACK_TIMEOUT_MAX: Duration = Duration::from_secs(3600);

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub ack_timeout: Duration,
    pub max_inflight: NonZeroUsize,
    pub max_resync_rounds: NonZeroU32,
    pub low_latency: bool,
    pub validate_state: bool,
    pub require_supported_firmware: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(DEVICE_QUEUE_FRAMES).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            validate_state: true,
            require_supported_firmware: false,
        }
    }
}

impl ClientConfig {
    pub(crate) fn transport(self) -> TransportConfig {
        TransportConfig {
            ack_timeout: self.ack_timeout,
            max_inflight: self.max_inflight,
            max_resync_rounds: self.max_resync_rounds,
            low_latency: self.low_latency,
        }
    }

    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_inflight.get() > MAX_INFLIGHT {
            return Err(PayloadError::MaxInflightTooLarge { max: MAX_INFLIGHT }.into());
        }
        if self.ack_timeout.is_zero() || self.ack_timeout > ACK_TIMEOUT_MAX {
            return Err(PayloadError::AckTimeoutOutOfRange {
                max: ACK_TIMEOUT_MAX,
            }
            .into());
        }
        Ok(self)
    }
}
