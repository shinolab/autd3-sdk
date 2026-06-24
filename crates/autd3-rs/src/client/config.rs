use std::num::{NonZeroU32, NonZeroUsize};

use core_affinity::CoreId;
use thread_priority::ThreadPriority;

use crate::error::{Error, PayloadError};
use crate::protocol::MAX_IN_FLIGHT;

pub const MAX_DEVICES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub timeout_cycles: u32,
    pub max_inflight: NonZeroUsize,
    pub send_interval_cycles: NonZeroU32,
    pub max_resync_rounds: NonZeroU32,
    pub low_latency: bool,
    pub reset_resend_cycles: u32,
    pub rt_priority: Option<ThreadPriority>,
    pub rt_affinity: Option<CoreId>,
    pub validate_state: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(MAX_IN_FLIGHT).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        }
    }
}

impl ClientConfig {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_inflight.get() > MAX_IN_FLIGHT {
            return Err(Error::InvalidPayload(PayloadError::MaxInFlightTooLarge {
                max: MAX_IN_FLIGHT,
            }));
        }
        Ok(self)
    }
}
