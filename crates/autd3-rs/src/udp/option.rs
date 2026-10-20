use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::{Interface, simulator};

use super::error::UdpError;
use super::pacer::MIN_SEND_RATE_PERCENT;

#[derive(Clone, Debug, PartialEq)]
pub struct TransportOption {
    pub iface: Interface,
    pub recovery_reply_timeout: Duration,
    pub lost_timeout: Duration,
    pub control_request_timeout: Duration,
    pub enumeration_timeout: Duration,
    pub sync_timeout: Duration,
    pub ack_timeout: Duration,
    pub send_rate_limit_percent: Option<f32>,
    pub send_buffer_bytes: Option<NonZeroUsize>,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            recovery_reply_timeout: Duration::from_millis(1),
            lost_timeout: Duration::from_millis(100),
            control_request_timeout: Duration::from_millis(200),
            enumeration_timeout: Duration::from_secs(10),
            sync_timeout: Duration::from_secs(30),
            ack_timeout: Duration::from_millis(10),
            send_rate_limit_percent: None,
            send_buffer_bytes: NonZeroUsize::new(8 * 1024),
        }
    }
}

impl TransportOption {
    pub fn validate(&self) -> Result<(), UdpError> {
        if self.recovery_reply_timeout.is_zero() {
            return Err(UdpError::ZeroRecoveryReplyTimeout);
        }
        if self.lost_timeout.is_zero() {
            return Err(UdpError::ZeroLostTimeout);
        }
        if self.control_request_timeout.is_zero() {
            return Err(UdpError::ZeroControlRequestTimeout);
        }
        if self.ack_timeout.is_zero() {
            return Err(UdpError::ZeroAckTimeout);
        }
        if let Interface::Simulator(port) = self.iface
            && simulator::control_port(port).is_none()
        {
            return Err(UdpError::InvalidSimulatorPort(port));
        }
        if let Some(percent) = self.send_rate_limit_percent
            && !(MIN_SEND_RATE_PERCENT..=100.0).contains(&percent)
        {
            return Err(UdpError::InvalidSendRateLimit {
                value: percent,
                min: MIN_SEND_RATE_PERCENT,
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
        assert_eq!(option.recovery_reply_timeout, Duration::from_millis(1));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.control_request_timeout, Duration::from_millis(200));
        assert_eq!(option.enumeration_timeout, Duration::from_secs(10));
        assert_eq!(option.sync_timeout, Duration::from_secs(30));
        assert_eq!(option.ack_timeout, Duration::from_millis(10));
        assert_eq!(option.send_rate_limit_percent, None);
        assert_eq!(option.send_buffer_bytes, NonZeroUsize::new(8192));
        assert!(option.validate().is_ok());
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[case(44336, true)]
    #[case(65534, true)]
    #[case(65535, false)]
    fn the_simulator_port_needs_room_for_the_control_port(#[case] port: u16, #[case] valid: bool) {
        let option = TransportOption {
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
        let with = |percent: f32| TransportOption {
            send_rate_limit_percent: Some(percent),
            ..TransportOption::default()
        };
        assert!(with(MIN_SEND_RATE_PERCENT).validate().is_ok());
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
    #[case::zero_response_timeout(
        TransportOption {
            control_request_timeout: Duration::ZERO,
            ..TransportOption::default()
        },
        |e: &UdpError| matches!(e, UdpError::ZeroControlRequestTimeout)
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
            recovery_reply_timeout: Duration::MAX,
            lost_timeout: Duration::MAX,
            control_request_timeout: Duration::MAX,
            enumeration_timeout: Duration::MAX,
            sync_timeout: Duration::MAX,
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

    #[test]
    fn a_zero_enumeration_timeout_is_allowed() {
        let option = TransportOption {
            enumeration_timeout: Duration::ZERO,
            ..TransportOption::default()
        };
        assert!(option.validate().is_ok());
    }
}
