use std::time::Duration;

use autd3_rs_core::Interface;

use super::SoemLinkOption;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoemLinkOptionFull {
    pub interface: Interface,
    pub sync0_period: Duration,
    pub sync0_shift: Duration,
    pub send_cycle: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
    pub state_timeout: Duration,
    pub op_wait_timeout: Duration,
}

impl SoemLinkOptionFull {
    #[must_use]
    pub fn safe_default() -> Self {
        SoemLinkOption::safe_default().into()
    }

    #[must_use]
    pub fn performance_default() -> Self {
        SoemLinkOption::performance_default().into()
    }
}

impl Default for SoemLinkOptionFull {
    fn default() -> Self {
        SoemLinkOption::default().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_default_uses_full_cycle_shift() {
        let opt = SoemLinkOptionFull::safe_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, opt.sync0_period);
    }

    #[test]
    fn performance_default_uses_zero_shift() {
        let opt = SoemLinkOptionFull::performance_default();
        assert_eq!(opt.sync0_period, Duration::from_millis(1));
        assert_eq!(opt.sync0_shift, Duration::ZERO);
    }
}
