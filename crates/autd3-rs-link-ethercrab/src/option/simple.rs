use std::time::Duration;

use autd3_rs_core::Interface;
use ethercrab::{MainDeviceConfig, RetryBehaviour, Timeouts, subdevice_group::DcConfiguration};

use super::EtherCrabLinkOptionFull;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtherCrabLinkOption {
    pub interface: Interface,
    pub sync0_period: Duration,
    pub sync0_shift: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
}

impl EtherCrabLinkOption {
    #[must_use]
    pub fn safe_default() -> Self {
        let sync0_period = Duration::from_millis(1);
        Self {
            interface: Interface::Auto,
            sync0_period,
            sync0_shift: sync0_period,
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
        }
    }

    #[must_use]
    pub fn performance_default() -> Self {
        Self {
            interface: Interface::Auto,
            sync0_period: Duration::from_millis(1),
            sync0_shift: Duration::ZERO,
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
        }
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
            Self::performance_default()
        }
    }
}

impl From<EtherCrabLinkOption> for EtherCrabLinkOptionFull {
    fn from(opt: EtherCrabLinkOption) -> Self {
        Self {
            interface: opt.interface,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_default_uses_full_cycle_shift() {
        let opt = EtherCrabLinkOption::safe_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, opt.sync0_period);
    }

    #[test]
    fn performance_default_uses_zero_shift() {
        let opt = EtherCrabLinkOption::performance_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, Duration::ZERO);
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
            EtherCrabLinkOption::performance_default()
        );
    }
}
