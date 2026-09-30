use std::net::SocketAddrV6;
use std::time::Duration;

use autd3_cpu_wire::udp::FAILSAFE_TIMEOUT_MS;
use autd3_rs_core::Interface;

use super::error::UdpError;

const MAX_DURATION: Duration = Duration::from_secs(3600);
const HEARTBEAT_MAX: Duration = Duration::from_millis(FAILSAFE_TIMEOUT_MS as u64 / 2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportOption {
    pub iface: Interface,
    pub group: Option<SocketAddrV6>,
    pub heartbeat: Duration,
    pub reply_timeout: Duration,
    pub lost_timeout: Duration,
    pub response_timeout: Duration,
    pub enumeration_timeout: Duration,
    pub sync_timeout: Duration,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            group: None,
            heartbeat: Duration::from_millis(10),
            reply_timeout: Duration::from_millis(1),
            lost_timeout: Duration::from_millis(100),
            response_timeout: Duration::from_millis(200),
            enumeration_timeout: Duration::from_secs(10),
            sync_timeout: Duration::from_secs(10),
        }
    }
}

impl TransportOption {
    pub fn validate(&self) -> Result<(), UdpError> {
        fn check(field: &'static str, value: Duration, min: Duration) -> Result<(), UdpError> {
            if value < min || value > MAX_DURATION {
                return Err(UdpError::InvalidOption {
                    field,
                    value,
                    min,
                    max: MAX_DURATION,
                });
            }
            Ok(())
        }

        let tick = Duration::from_micros(1);
        check("heartbeat", self.heartbeat, tick)?;
        if self.heartbeat >= HEARTBEAT_MAX {
            return Err(UdpError::InvalidOption {
                field: "heartbeat",
                value: self.heartbeat,
                min: tick,
                max: HEARTBEAT_MAX.saturating_sub(tick),
            });
        }
        check("reply_timeout", self.reply_timeout, tick)?;
        check("lost_timeout", self.lost_timeout, self.heartbeat + tick)?;
        check("response_timeout", self.response_timeout, tick)?;
        check(
            "enumeration_timeout",
            self.enumeration_timeout,
            Duration::ZERO,
        )?;
        check("sync_timeout", self.sync_timeout, Duration::ZERO)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_follow_the_spec() {
        let option = TransportOption::default();
        assert_eq!(option.iface, Interface::Auto);
        assert_eq!(option.group, None);
        assert_eq!(option.heartbeat, Duration::from_millis(10));
        assert_eq!(option.reply_timeout, Duration::from_millis(1));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.response_timeout, Duration::from_millis(200));
        assert_eq!(option.enumeration_timeout, Duration::from_secs(10));
        assert_eq!(option.sync_timeout, Duration::from_secs(10));
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_zero_heartbeat_is_rejected() {
        let option = TransportOption {
            heartbeat: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "heartbeat",
                ..
            })
        ));
    }

    #[test]
    fn a_heartbeat_that_would_let_the_failsafe_fire_is_rejected() {
        let option = TransportOption {
            heartbeat: Duration::from_millis(250),
            lost_timeout: Duration::from_secs(1),
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "heartbeat",
                ..
            })
        ));
        let option = TransportOption {
            heartbeat: Duration::from_millis(249),
            lost_timeout: Duration::from_secs(1),
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn the_lost_timeout_must_outlast_the_heartbeat() {
        let option = TransportOption {
            heartbeat: Duration::from_millis(20),
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
    fn a_zero_reply_timeout_is_rejected() {
        let option = TransportOption {
            reply_timeout: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "reply_timeout",
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

    #[test]
    fn an_absurd_response_timeout_is_rejected() {
        let option = TransportOption {
            response_timeout: Duration::from_secs(7200),
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption {
                field: "response_timeout",
                ..
            })
        ));
    }
}
