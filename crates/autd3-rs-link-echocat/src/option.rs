use std::time::Duration;

use autd3_rs_core::Interface;

use crate::master::{MasterConfig, SleepStrategy};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchocatLinkOption {
    pub iface: Interface,
    pub sync0_period: Duration,
    pub pdu_timeout: Duration,
    pub state_transition_timeout: Duration,
    pub dc_static_sync_iterations: u32,
    pub dc_start_delay: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
    pub process_data_watchdog: Duration,
    pub sleep_strategy: SleepStrategy,
}

#[cfg(target_os = "windows")]
const DEFAULT_SYNC0_PERIOD: Duration = Duration::from_millis(2);
#[cfg(not(target_os = "windows"))]
const DEFAULT_SYNC0_PERIOD: Duration = Duration::from_millis(1);

impl Default for EchocatLinkOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            sync0_period: DEFAULT_SYNC0_PERIOD,
            pdu_timeout: Duration::from_millis(100),
            state_transition_timeout: Duration::from_secs(10),
            dc_static_sync_iterations: 10_000,
            dc_start_delay: Duration::from_millis(100),
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
            process_data_watchdog: Duration::from_millis(100),
            sleep_strategy: SleepStrategy::Sleep,
        }
    }
}

impl From<&EchocatLinkOption> for MasterConfig {
    fn from(option: &EchocatLinkOption) -> Self {
        Self {
            cycle: option.sync0_period,
            pdu_timeout: option.pdu_timeout,
            state_transition_timeout: option.state_transition_timeout,
            dc_static_sync_iterations: option.dc_static_sync_iterations,
            dc_start_delay: option.dc_start_delay,
            sync_tolerance: option.sync_tolerance,
            sync_timeout: option.sync_timeout,
            process_data_watchdog: option.process_data_watchdog,
            sleep_strategy: option.sleep_strategy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EchocatLinkOption, MasterConfig, SleepStrategy};
    use std::time::Duration;

    #[test]
    fn the_default_wait_never_burns_a_core() {
        let config = MasterConfig::from(&EchocatLinkOption::default());
        assert_eq!(
            config.sleep_strategy,
            SleepStrategy::Sleep,
            "spinning costs 92% of a core and buys only landing-phase precision the subdevice ignores",
        );
    }

    #[test]
    fn the_chosen_wait_reaches_the_master() {
        let option = EchocatLinkOption {
            sleep_strategy: SleepStrategy::Spin {
                margin: Duration::from_millis(1),
            },
            ..EchocatLinkOption::default()
        };
        assert_eq!(
            MasterConfig::from(&option).sleep_strategy,
            SleepStrategy::Spin {
                margin: Duration::from_millis(1)
            },
        );
    }

    #[test]
    fn the_sync0_period_drives_the_master_cycle() {
        let option = EchocatLinkOption::default();
        assert_eq!(MasterConfig::from(&option).cycle, option.sync0_period);
    }

    #[test]
    fn windows_gets_a_slower_default_period() {
        let period = EchocatLinkOption::default().sync0_period;
        #[cfg(target_os = "windows")]
        assert_eq!(
            period,
            Duration::from_millis(2),
            "at 1 ms the RT thread's wake jitter crosses the SYNC0 edge and the subdevice \
             drops to SAFE-OP with a synchronization error (0x001A)",
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(period, Duration::from_millis(1));
    }
}
