use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use crate::error::{Error, PayloadError};
use crate::protocol::MAX_INFLIGHT;

pub use crate::udp::bus::MAX_DEVICES;

const EXEC_LEAD_MIN: Duration = Duration::from_millis(1);

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    pub ack_timeout: Duration,
    pub max_inflight: NonZeroUsize,
    pub max_resync_rounds: NonZeroU32,
    pub require_supported_firmware: bool,
    pub resynchronize_on_recovery: Option<NonZeroU32>,
    pub release_failsafe_on_recovery: Option<NonZeroU32>,
    pub exec_lead: Option<Duration>,
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
            exec_lead: None,
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
        if let Some(exec_lead) = self.exec_lead
            && exec_lead < EXEC_LEAD_MIN
        {
            return Err(PayloadError::ExecLeadTooShort {
                got: exec_lead,
                min: EXEC_LEAD_MIN,
            }
            .into());
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
    fn exec_lead_is_off_by_default() {
        assert_eq!(ClientConfig::default().exec_lead, None);
    }

    #[test]
    fn an_exec_lead_below_one_millisecond_is_rejected() {
        let with = |exec_lead| ClientConfig {
            exec_lead: Some(exec_lead),
            ..ClientConfig::default()
        };
        assert!(with(EXEC_LEAD_MIN).validate().is_ok());
        assert!(matches!(
            with(Duration::from_micros(999)).validate(),
            Err(Error::InvalidPayload(PayloadError::ExecLeadTooShort { .. }))
        ));
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
