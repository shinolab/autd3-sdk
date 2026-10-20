use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::{Interface, simulator};

use super::error::UdpError;
use super::pacer::MIN_PERCENT;

#[derive(Clone, Debug, PartialEq)]
pub struct UdpOption {
    pub iface: Interface,
    pub heartbeat: Option<Duration>,
    pub reply_timeout: Duration,
    pub lost_timeout: Duration,
    pub recovery_interval: Option<Duration>,
    pub response_timeout: Duration,
    pub enumeration_timeout: Duration,
    pub sync_timeout: Duration,
    pub send_rate_limit: Option<f32>,
    pub send_buffer: Option<NonZeroUsize>,
    pub timer_resolution: Option<Duration>,
}

impl Default for UdpOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            heartbeat: Some(Duration::from_millis(10)),
            reply_timeout: Duration::from_millis(1),
            lost_timeout: Duration::from_millis(100),
            recovery_interval: None,
            response_timeout: Duration::from_millis(200),
            enumeration_timeout: Duration::from_secs(10),
            sync_timeout: Duration::from_secs(30),
            send_rate_limit: None,
            send_buffer: NonZeroUsize::new(8 * 1024),
            timer_resolution: Some(Duration::from_millis(1)),
        }
    }
}

impl UdpOption {
    pub fn validate(&self) -> Result<(), UdpError> {
        if self.heartbeat.is_some_and(|heartbeat| heartbeat.is_zero()) {
            return Err(UdpError::ZeroHeartbeat);
        }
        if self.reply_timeout.is_zero() {
            return Err(UdpError::ZeroReplyTimeout);
        }
        if self.lost_timeout <= self.heartbeat.unwrap_or_default() {
            return Err(UdpError::LostTimeoutNotAboveHeartbeat {
                lost_timeout: self.lost_timeout,
                heartbeat: self.heartbeat,
            });
        }
        if self
            .recovery_interval
            .is_some_and(|recovery_interval| recovery_interval.is_zero())
        {
            return Err(UdpError::ZeroRecoveryInterval);
        }
        if self.response_timeout.is_zero() {
            return Err(UdpError::ZeroResponseTimeout);
        }
        let min = Duration::from_millis(1);
        if let Some(got) = self.timer_resolution
            && got < min
        {
            return Err(UdpError::TimerResolutionTooShort { got, min });
        }
        if let Interface::Simulator(port) = self.iface
            && simulator::control_port(port).is_none()
        {
            return Err(UdpError::InvalidSimulatorPort(port));
        }
        if let Some(percent) = self.send_rate_limit
            && !(MIN_PERCENT..=100.0).contains(&percent)
        {
            return Err(UdpError::InvalidSendRateLimit {
                value: percent,
                min: MIN_PERCENT,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn the_defaults_follow_the_spec() {
        let option = UdpOption::default();
        assert_eq!(option.iface, Interface::Auto);
        assert_eq!(option.heartbeat, Some(Duration::from_millis(10)));
        assert_eq!(option.reply_timeout, Duration::from_millis(1));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.recovery_interval, None);
        assert_eq!(option.response_timeout, Duration::from_millis(200));
        assert_eq!(option.enumeration_timeout, Duration::from_secs(10));
        assert_eq!(option.sync_timeout, Duration::from_secs(30));
        assert_eq!(option.send_rate_limit, None);
        assert_eq!(option.send_buffer, NonZeroUsize::new(8192));
        assert_eq!(option.timer_resolution, Some(Duration::from_millis(1)));
        assert!(option.validate().is_ok());
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[case(44336, true)]
    #[case(65534, true)]
    #[case(65535, false)]
    fn the_simulator_port_needs_room_for_the_control_port(#[case] port: u16, #[case] valid: bool) {
        let option = UdpOption {
            iface: Interface::Simulator(port),
            ..Default::default()
        };
        match option.validate() {
            Ok(()) => assert!(valid),
            Err(UdpError::InvalidSimulatorPort(p)) => assert!(!valid && p == port),
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn a_send_rate_limit_outside_the_percent_range_is_rejected() {
        let with = |percent: f32| UdpOption {
            send_rate_limit: Some(percent),
            ..UdpOption::default()
        };
        assert!(with(MIN_PERCENT).validate().is_ok());
        assert!(with(95.0).validate().is_ok());
        assert!(with(100.0).validate().is_ok());
        for rejected in [0.0, 0.95, 3.0, 100.1, f32::NAN, f32::INFINITY] {
            assert!(matches!(
                with(rejected).validate(),
                Err(UdpError::InvalidSendRateLimit { .. })
            ));
        }
    }

    #[rstest]
    #[case::zero_heartbeat(
        UdpOption {
            heartbeat: Some(Duration::ZERO),
            ..UdpOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroHeartbeat)
    )]
    #[case::zero_reply_timeout(
        UdpOption {
            reply_timeout: Duration::ZERO,
            ..UdpOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroReplyTimeout)
    )]
    #[case::zero_recovery_interval(
        UdpOption {
            recovery_interval: Some(Duration::ZERO),
            ..UdpOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroRecoveryInterval)
    )]
    #[case::zero_response_timeout(
        UdpOption {
            response_timeout: Duration::ZERO,
            ..UdpOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroResponseTimeout)
    )]
    #[case::sub_millisecond_timer_resolution(
        UdpOption {
            timer_resolution: Some(Duration::from_micros(999)),
            ..UdpOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::TimerResolutionTooShort { .. })
    )]
    fn a_duration_at_or_below_its_bound_is_rejected(
        #[case] option: UdpOption,
        #[case] is_expected: fn(&UdpError) -> bool,
    ) {
        assert!(option.validate().is_err_and(|e| is_expected(&e)));
    }

    #[test]
    fn the_durations_have_no_upper_bound() {
        let option = UdpOption {
            heartbeat: Duration::MAX.checked_sub(Duration::from_nanos(1)),
            reply_timeout: Duration::MAX,
            lost_timeout: Duration::MAX,
            recovery_interval: Some(Duration::MAX),
            response_timeout: Duration::MAX,
            enumeration_timeout: Duration::MAX,
            sync_timeout: Duration::MAX,
            timer_resolution: Some(Duration::MAX),
            ..UdpOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_heartbeat_longer_than_the_default_failsafe_is_allowed() {
        let option = UdpOption {
            heartbeat: Some(Duration::from_secs(1)),
            lost_timeout: Duration::from_secs(2),
            ..UdpOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_disabled_heartbeat_is_allowed() {
        let option = UdpOption {
            heartbeat: None,
            lost_timeout: Duration::from_nanos(1),
            ..UdpOption::default()
        };
        assert!(option.validate().is_ok());
        let option = UdpOption {
            heartbeat: None,
            lost_timeout: Duration::ZERO,
            ..UdpOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::LostTimeoutNotAboveHeartbeat { .. })
        ));
    }

    #[test]
    fn a_disabled_timer_resolution_is_allowed() {
        let option = UdpOption {
            timer_resolution: None,
            ..UdpOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn the_lost_timeout_must_outlast_the_heartbeat() {
        let option = UdpOption {
            heartbeat: Some(Duration::from_millis(20)),
            lost_timeout: Duration::from_millis(20),
            ..UdpOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::LostTimeoutNotAboveHeartbeat { .. })
        ));
    }

    #[test]
    fn a_zero_enumeration_timeout_is_allowed() {
        let option = UdpOption {
            enumeration_timeout: Duration::ZERO,
            ..UdpOption::default()
        };
        assert!(option.validate().is_ok());
    }
}
