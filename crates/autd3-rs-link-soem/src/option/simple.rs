use std::time::Duration;

use autd3_rs_core::Interface;

use super::SoemLinkOptionFull;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoemLinkOption {
    pub interface: Interface,
    pub sync0_period: Duration,
    pub sync0_shift: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
}

impl SoemLinkOption {
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

impl Default for SoemLinkOption {
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

impl From<SoemLinkOption> for SoemLinkOptionFull {
    fn from(opt: SoemLinkOption) -> Self {
        Self {
            interface: opt.interface,
            sync0_period: opt.sync0_period,
            sync0_shift: opt.sync0_shift,
            send_cycle: opt.sync0_period,
            sync_tolerance: opt.sync_tolerance,
            sync_timeout: opt.sync_timeout,
            state_timeout: Duration::from_secs(10),
            op_wait_timeout: Duration::from_secs(10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_default_uses_full_cycle_shift() {
        let opt = SoemLinkOption::safe_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, opt.sync0_period);
    }

    #[test]
    fn performance_default_uses_zero_shift() {
        let opt = SoemLinkOption::performance_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, Duration::ZERO);
    }

    #[test]
    fn default_matches_target_preset() {
        #[cfg(target_os = "windows")]
        assert_eq!(SoemLinkOption::default(), SoemLinkOption::safe_default());
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            SoemLinkOption::default(),
            SoemLinkOption::performance_default()
        );
    }
}
