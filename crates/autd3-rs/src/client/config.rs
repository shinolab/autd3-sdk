use std::num::{NonZeroU32, NonZeroUsize};

use autd3_rs_core::RtSchedulePolicy;
use core_affinity::CoreId;
use thread_priority::ThreadPriority;

use crate::error::{Error, PayloadError};
use crate::protocol::MAX_INFLIGHT;

pub const MAX_DEVICES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub timeout_cycles: u32,
    pub max_inflight: NonZeroUsize,
    pub max_resync_rounds: NonZeroU32,
    pub low_latency: bool,
    pub reset_resend_cycles: NonZeroU32,
    pub rt_priority: Option<ThreadPriority>,
    pub rt_policy: RtSchedulePolicy,
    pub rt_affinity: Option<CoreId>,
    pub validate_state: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: NonZeroU32::new(2).unwrap(),
            rt_priority: autd3_rs_core::default_rt_priority(),
            rt_policy: RtSchedulePolicy::default(),
            rt_affinity: None,
            validate_state: true,
        }
    }
}

impl ClientConfig {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_inflight.get() > MAX_INFLIGHT {
            return Err(PayloadError::MaxInflightTooLarge { max: MAX_INFLIGHT }.into());
        }
        Ok(self)
    }
}
