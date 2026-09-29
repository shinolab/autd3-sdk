use std::net::SocketAddrV6;
use std::time::Duration;

use autd3_rs_core::Interface;

use super::error::UdpError;

const MAX_DURATION: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportOption {
    pub iface: Interface,
    pub group: Option<SocketAddrV6>,
    pub cycle: Duration,
    pub reply_timeout: Duration,
    pub response_timeout: Duration,
    pub enumeration_timeout: Duration,
    pub sync_timeout: Duration,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            group: None,
            cycle: Duration::from_millis(1),
            reply_timeout: Duration::from_millis(1),
            response_timeout: Duration::from_millis(200),
            enumeration_timeout: Duration::from_secs(10),
            sync_timeout: Duration::from_secs(5),
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
        check("cycle", self.cycle, tick)?;
        check("reply_timeout", self.reply_timeout, tick)?;
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
        assert_eq!(option.cycle, Duration::from_millis(1));
        assert_eq!(option.reply_timeout, Duration::from_millis(1));
        assert_eq!(option.response_timeout, Duration::from_millis(200));
        assert_eq!(option.enumeration_timeout, Duration::from_secs(10));
        assert_eq!(option.sync_timeout, Duration::from_secs(5));
        assert!(option.validate().is_ok());
    }

    #[test]
    fn a_zero_cycle_is_rejected() {
        let option = TransportOption {
            cycle: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(UdpError::InvalidOption { field: "cycle", .. })
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
