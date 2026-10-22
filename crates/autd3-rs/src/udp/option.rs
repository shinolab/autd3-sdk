use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::Interface;

use super::error::UdpError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportOption {
    pub iface: Interface,
    pub open_timeout: Duration,
    pub recovery_reply_timeout: Duration,
    pub lost_timeout: Duration,
    pub ack_timeout: Duration,
    pub send_buffer_bytes: Option<NonZeroUsize>,
    pub sync_on_open: bool,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            open_timeout: Duration::from_secs(40),
            recovery_reply_timeout: Duration::from_millis(1),
            lost_timeout: Duration::from_millis(100),
            ack_timeout: Duration::from_millis(10),
            send_buffer_bytes: NonZeroUsize::new(8 * 1024),
            sync_on_open: true,
        }
    }
}

impl TransportOption {
    pub fn validate(&self) -> Result<(), UdpError> {
        if self.open_timeout.is_zero() {
            return Err(UdpError::ZeroOpenTimeout);
        }
        if self.recovery_reply_timeout.is_zero() {
            return Err(UdpError::ZeroRecoveryReplyTimeout);
        }
        if self.lost_timeout.is_zero() {
            return Err(UdpError::ZeroLostTimeout);
        }
        if self.ack_timeout.is_zero() {
            return Err(UdpError::ZeroAckTimeout);
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
        assert_eq!(option.open_timeout, Duration::from_secs(40));
        assert_eq!(option.recovery_reply_timeout, Duration::from_millis(1));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.ack_timeout, Duration::from_millis(10));
        assert_eq!(option.send_buffer_bytes, NonZeroUsize::new(8192));
        assert!(option.sync_on_open);
        assert!(option.validate().is_ok());
    }

    #[rstest]
    #[case::zero_open_timeout(
        TransportOption {
            open_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroOpenTimeout)
    )]
    #[case::zero_reply_timeout(
        TransportOption {
            recovery_reply_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroRecoveryReplyTimeout)
    )]
    #[case::zero_lost_timeout(
        TransportOption {
            lost_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroLostTimeout)
    )]
    #[case::zero_ack_timeout(
        TransportOption {
            ack_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroAckTimeout)
    )]
    fn a_duration_at_or_below_its_bound_is_rejected(
        #[case] option: TransportOption,
        #[case] is_expected: fn(&UdpError) -> bool,
    ) {
        assert!(option.validate().is_err_and(|e| is_expected(&e)));
    }

    #[test]
    fn the_durations_have_no_upper_bound() {
        let option = TransportOption {
            open_timeout: Duration::MAX,
            recovery_reply_timeout: Duration::MAX,
            lost_timeout: Duration::MAX,
            ack_timeout: Duration::MAX,
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }

    #[test]
    fn the_shortest_lost_timeout_is_allowed() {
        let option = TransportOption {
            lost_timeout: Duration::from_nanos(1),
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }
}
