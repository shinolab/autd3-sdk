use std::time::Duration;

use autd3_rs_core::Interface;

use crate::error::EchocatError;
use crate::master::{FramePhase, MasterConfig, SleepStrategy};

pub const MAX_SYNC0_PERIOD: Duration = Duration::from_nanos(u32::MAX as u64);
pub const MAX_SYNC_TOLERANCE: Duration = Duration::from_nanos(u32::MAX as u64);
pub const MAX_DC_START_DELAY: Duration = Duration::from_nanos(u32::MAX as u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchocatLinkOption {
    pub iface: Interface,
    pub sync0_period: Duration,
    pub frame_phase: FramePhase,
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
            frame_phase: FramePhase::Auto,
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

impl EchocatLinkOption {
    pub fn validate(&self) -> Result<(), EchocatError> {
        fn check(
            field: &'static str,
            value: Duration,
            min: Duration,
            max: Duration,
        ) -> Result<(), EchocatError> {
            if value < min || value > max {
                return Err(EchocatError::InvalidOption {
                    field,
                    value,
                    min,
                    max,
                });
            }
            Ok(())
        }

        check(
            "sync0_period",
            self.sync0_period,
            Duration::from_nanos(1),
            MAX_SYNC0_PERIOD,
        )?;
        check(
            "sync_tolerance",
            self.sync_tolerance,
            Duration::ZERO,
            MAX_SYNC_TOLERANCE,
        )?;
        check(
            "dc_start_delay",
            self.dc_start_delay,
            Duration::ZERO,
            MAX_DC_START_DELAY,
        )?;
        check(
            "pdu_timeout",
            self.pdu_timeout,
            Duration::from_nanos(1),
            Duration::from_secs(60),
        )?;
        Ok(())
    }
}

impl From<&EchocatLinkOption> for MasterConfig {
    fn from(option: &EchocatLinkOption) -> Self {
        Self {
            cycle: option.sync0_period,
            frame_phase: option.frame_phase,
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
    use super::{EchocatError, EchocatLinkOption, FramePhase, MasterConfig, SleepStrategy};
    use std::time::Duration;

    #[test]
    fn the_default_option_validates() {
        assert!(EchocatLinkOption::default().validate().is_ok());
    }

    #[test]
    fn a_zero_sync0_period_is_rejected_before_it_divides_by_zero() {
        let option = EchocatLinkOption {
            sync0_period: Duration::ZERO,
            ..EchocatLinkOption::default()
        };
        assert!(matches!(
            option.validate(),
            Err(EchocatError::InvalidOption {
                field: "sync0_period",
                ..
            })
        ));
    }

    #[test]
    fn a_sync0_period_beyond_u32_nanoseconds_is_rejected() {
        assert!(
            EchocatLinkOption {
                sync0_period: super::MAX_SYNC0_PERIOD,
                ..EchocatLinkOption::default()
            }
            .validate()
            .is_ok()
        );
        assert!(matches!(
            EchocatLinkOption {
                sync0_period: super::MAX_SYNC0_PERIOD + Duration::from_nanos(1),
                ..EchocatLinkOption::default()
            }
            .validate(),
            Err(EchocatError::InvalidOption {
                field: "sync0_period",
                ..
            })
        ));
    }

    #[test]
    fn an_out_of_range_sync_tolerance_is_rejected() {
        assert!(matches!(
            EchocatLinkOption {
                sync_tolerance: Duration::from_secs(5),
                ..EchocatLinkOption::default()
            }
            .validate(),
            Err(EchocatError::InvalidOption {
                field: "sync_tolerance",
                ..
            })
        ));
    }

    #[test]
    fn an_out_of_range_dc_start_delay_is_rejected() {
        assert!(matches!(
            EchocatLinkOption {
                dc_start_delay: Duration::from_secs(5),
                ..EchocatLinkOption::default()
            }
            .validate(),
            Err(EchocatError::InvalidOption {
                field: "dc_start_delay",
                ..
            })
        ));
    }

    #[test]
    fn a_zero_pdu_timeout_is_rejected() {
        assert!(matches!(
            EchocatLinkOption {
                pdu_timeout: Duration::ZERO,
                ..EchocatLinkOption::default()
            }
            .validate(),
            Err(EchocatError::InvalidOption {
                field: "pdu_timeout",
                ..
            })
        ));
    }

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
    fn the_landing_phase_follows_the_exchange_by_default() {
        assert_eq!(
            EchocatLinkOption::default().frame_phase,
            FramePhase::Auto,
            "a fixed mid-period landing leaves 20 devices no room to finish before SYNC0",
        );
    }

    #[test]
    fn the_chosen_landing_phase_reaches_the_master() {
        let option = EchocatLinkOption {
            frame_phase: FramePhase::At(Duration::from_micros(500)),
            ..EchocatLinkOption::default()
        };
        assert_eq!(
            MasterConfig::from(&option).frame_phase,
            FramePhase::At(Duration::from_micros(500)),
        );
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
