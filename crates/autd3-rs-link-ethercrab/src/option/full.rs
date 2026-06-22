use std::time::Duration;

use autd3_rs_core::Interface;
use ethercrab::{MainDeviceConfig, Timeouts, subdevice_group::DcConfiguration};

use super::EtherCrabLinkOption;

#[derive(Clone, Debug)]
pub struct EtherCrabLinkOptionFull {
    pub interface: Interface,
    pub timeouts: Timeouts,
    pub main_device_config: MainDeviceConfig,
    pub dc_configuration: DcConfiguration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
}

impl EtherCrabLinkOptionFull {
    #[must_use]
    pub fn safe_default() -> Self {
        EtherCrabLinkOption::safe_default().into()
    }

    #[must_use]
    pub fn performance_default() -> Self {
        EtherCrabLinkOption::performance_default().into()
    }
}

impl Default for EtherCrabLinkOptionFull {
    fn default() -> Self {
        EtherCrabLinkOption::default().into()
    }
}

impl PartialEq for EtherCrabLinkOptionFull {
    fn eq(&self, other: &Self) -> bool {
        let Timeouts {
            state_transition,
            pdu,
            eeprom,
            wait_loop_delay,
            mailbox_echo,
            mailbox_response,
        } = self.timeouts;
        let DcConfiguration {
            start_delay,
            sync0_period,
            sync0_shift,
        } = self.dc_configuration;
        self.interface == other.interface
            && state_transition == other.timeouts.state_transition
            && pdu == other.timeouts.pdu
            && eeprom == other.timeouts.eeprom
            && wait_loop_delay == other.timeouts.wait_loop_delay
            && mailbox_echo == other.timeouts.mailbox_echo
            && mailbox_response == other.timeouts.mailbox_response
            && self.main_device_config == other.main_device_config
            && start_delay == other.dc_configuration.start_delay
            && sync0_period == other.dc_configuration.sync0_period
            && sync0_shift == other.dc_configuration.sync0_shift
            && self.sync_tolerance == other.sync_tolerance
            && self.sync_timeout == other.sync_timeout
    }
}

impl Eq for EtherCrabLinkOptionFull {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_default_uses_full_cycle_shift() {
        let opt = EtherCrabLinkOptionFull::safe_default();
        assert_eq!(opt.dc_configuration.sync0_period, Duration::from_millis(1));
        assert_eq!(
            opt.dc_configuration.sync0_shift,
            opt.dc_configuration.sync0_period
        );
    }

    #[test]
    fn performance_default_uses_zero_shift() {
        let opt = EtherCrabLinkOptionFull::performance_default();
        assert_eq!(opt.dc_configuration.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.dc_configuration.sync0_shift, Duration::ZERO);
    }
}
