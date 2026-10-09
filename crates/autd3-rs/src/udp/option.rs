use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::Interface;

use super::error::UdpError;
use super::pacer::MIN_PERCENT;

#[derive(Clone, Debug, PartialEq)]
pub struct TransportOption {
    pub iface: Interface,
    pub heartbeat: Option<Duration>,
    pub reply_timeout: Duration,
    pub lost_timeout: Duration,
    pub response_timeout: Duration,
    pub enumeration_timeout: Duration,
    pub sync_timeout: Duration,
    pub send_rate_limit: Option<f32>,
    pub send_buffer: Option<NonZeroUsize>,
    pub timer_resolution: Option<Duration>,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            heartbeat: Some(Duration::from_millis(10)),
            reply_timeout: Duration::from_millis(1),
            lost_timeout: Duration::from_millis(100),
            response_timeout: Duration::from_millis(200),
            enumeration_timeout: Duration::from_secs(10),
            sync_timeout: Duration::from_secs(30),
            send_rate_limit: None,
            send_buffer: NonZeroUsize::new(8 * 1024),
            timer_resolution: Some(Duration::from_millis(1)),
        }
    }
}

impl TransportOption {
    pub fn validate(&self) -> Result<(), UdpError> {
        fn check(field: &'static str, value: Duration, min: Duration) -> Result<(), UdpError> {
            if value < min {
                return Err(UdpError::InvalidOption { field, value, min });
            }
            Ok(())
        }

        let tick = Duration::from_micros(1);
        if let Some(heartbeat) = self.heartbeat {
            check("heartbeat", heartbeat, tick)?;
        }
        check("reply_timeout", self.reply_timeout, tick)?;
        check(
            "lost_timeout",
            self.lost_timeout,
            self.heartbeat.unwrap_or_default().saturating_add(tick),
        )?;
        check("response_timeout", self.response_timeout, tick)?;
        if let Some(timer_resolution) = self.timer_resolution {
            check(
                "timer_resolution",
                timer_resolution,
                Duration::from_millis(1),
            )?;
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
        let option = TransportOption::default();
        assert_eq!(option.iface, Interface::Auto);
        assert_eq!(option.heartbeat, Some(Duration::from_millis(10)));
        assert_eq!(option.reply_timeout, Duration::from_millis(1));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.response_timeout, Duration::from_millis(200));
        assert_eq!(option.enumeration_timeout, Duration::from_secs(10));
        assert_eq!(option.sync_timeout, Duration::from_secs(30));
        assert_eq!(option.send_rate_limit, None);
        assert_eq!(option.send_buffer, NonZeroUsize::new(8192));
        assert_eq!(option.timer_resolution, Some(Duration::from_millis(1)));
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_send_rate_limit_outside_the_percent_range_is_rejected() {
        let with = |percent: f32| TransportOption {
            send_rate_limit: Some(percent),
            ..TransportOption::default()
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
        TransportOption {
            heartbeat: Some(Duration::ZERO),
            ..TransportOption::default()
        },
        "heartbeat"
    )]
    #[case::zero_reply_timeout(
        TransportOption {
            reply_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        "reply_timeout"
    )]
    #[case::zero_response_timeout(
        TransportOption {
            response_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        "response_timeout"
    )]
    #[case::sub_millisecond_timer_resolution(
        TransportOption {
            timer_resolution: Some(Duration::from_micros(999)),
            ..TransportOption::default()
        },
        "timer_resolution"
    )]
    fn a_duration_below_its_minimum_is_rejected(
        #[case] option: TransportOption,
        #[case] rejected: &str,
    ) {
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption { field, .. }) if field == rejected
        ));
    }

    #[test]
    fn the_durations_have_no_upper_bound() {
        let option = TransportOption {
            heartbeat: Some(Duration::MAX),
            reply_timeout: Duration::MAX,
            lost_timeout: Duration::MAX,
            response_timeout: Duration::MAX,
            enumeration_timeout: Duration::MAX,
            sync_timeout: Duration::MAX,
            timer_resolution: Some(Duration::MAX),
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_heartbeat_longer_than_the_default_failsafe_is_allowed() {
        let option = TransportOption {
            heartbeat: Some(Duration::from_secs(1)),
            lost_timeout: Duration::from_secs(2),
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_disabled_heartbeat_is_allowed() {
        let option = TransportOption {
            heartbeat: None,
            lost_timeout: Duration::from_micros(1),
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
        let option = TransportOption {
            heartbeat: None,
            lost_timeout: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "lost_timeout",
                ..
            })
        ));
    }

    #[test]
    fn a_disabled_timer_resolution_is_allowed() {
        let option = TransportOption {
            timer_resolution: None,
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn the_lost_timeout_must_outlast_the_heartbeat() {
        let option = TransportOption {
            heartbeat: Some(Duration::from_millis(20)),
            lost_timeout: Duration::from_millis(20),
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "lost_timeout",
                ..
            })
        ));
    }

    #[test]
    fn a_zero_enumeration_timeout_is_allowed() {
        let option = TransportOption {
            enumeration_timeout: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }
}
