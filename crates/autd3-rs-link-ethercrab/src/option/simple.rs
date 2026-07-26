use std::time::Duration;

use autd3_rs_core::Interface;
use ethercrab::{MainDeviceConfig, RetryBehaviour, Timeouts, subdevice_group::DcConfiguration};

use super::EtherCrabLinkOptionFull;
use crate::osal::thread::{RtSchedulePolicy, ThreadPriority, ThreadPriorityValue};

const PERFORMANCE_TX_RX_PRIORITY: u8 = 99;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtherCrabLinkOption {
    pub iface: Interface,
    pub sync0_period: Duration,
    pub sync0_shift: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
}

impl EtherCrabLinkOption {
    #[must_use]
    pub fn safe_default() -> Self {
        Self {
            iface: Interface::Auto,
            sync0_period: Duration::from_millis(2),
            sync0_shift: Duration::ZERO,
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
        }
    }

    fn performance_base() -> Self {
        Self {
            iface: Interface::Auto,
            sync0_period: Duration::from_millis(1),
            sync0_shift: Duration::ZERO,
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
        }
    }

    #[must_use]
    pub fn performance_default() -> EtherCrabLinkOptionFull {
        let mut full: EtherCrabLinkOptionFull = Self::performance_base().into();
        full.tx_rx_priority = Some(ThreadPriority::Crossplatform(
            ThreadPriorityValue::try_from(PERFORMANCE_TX_RX_PRIORITY)
                .expect("0..=99 is a valid thread priority"),
        ));
        full.tx_rx_policy = RtSchedulePolicy::Fifo;
        full
    }
}

impl Default for EtherCrabLinkOption {
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::safe_default()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::performance_base()
        }
    }
}

impl From<EtherCrabLinkOption> for EtherCrabLinkOptionFull {
    fn from(opt: EtherCrabLinkOption) -> Self {
        Self {
            iface: opt.iface,
            timeouts: Timeouts {
                state_transition: Duration::from_secs(10),
                pdu: Duration::from_millis(100),
                wait_loop_delay: Duration::ZERO,
                ..Default::default()
            },
            main_device_config: MainDeviceConfig {
                dc_static_sync_iterations: 10000,
                retry_behaviour: RetryBehaviour::None,
            },
            dc_configuration: DcConfiguration {
                start_delay: Duration::from_millis(100),
                sync0_period: opt.sync0_period,
                sync0_shift: opt.sync0_shift,
            },
            sync_tolerance: opt.sync_tolerance,
            sync_timeout: opt.sync_timeout,
            tx_rx_priority: None,
            tx_rx_policy: autd3_rs_core::RtSchedulePolicy::default(),
            tx_rx_affinity: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_default_uses_2ms_cycle_and_zero_shift() {
        let opt = EtherCrabLinkOption::safe_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(2));
        assert_eq!(opt.sync0_shift, Duration::ZERO);
    }

    #[test]
    fn performance_default_uses_zero_shift_and_rt_pump() {
        let opt = EtherCrabLinkOption::performance_default();
        assert_eq!(opt.dc_configuration.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.dc_configuration.sync0_shift, Duration::ZERO);
        assert_eq!(
            opt.tx_rx_priority,
            Some(ThreadPriority::Crossplatform(
                ThreadPriorityValue::try_from(PERFORMANCE_TX_RX_PRIORITY).unwrap()
            ))
        );
        assert_eq!(opt.tx_rx_policy, RtSchedulePolicy::Fifo);
    }

    #[test]
    fn default_matches_target_preset() {
        #[cfg(target_os = "windows")]
        assert_eq!(
            EtherCrabLinkOption::default(),
            EtherCrabLinkOption::safe_default()
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            EtherCrabLinkOption::default(),
            EtherCrabLinkOption::performance_base()
        );
    }
}
