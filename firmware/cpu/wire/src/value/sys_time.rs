use core::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SysTime(u64);

impl SysTime {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_nanos(ns: u64) -> Self {
        Self(ns)
    }

    #[must_use]
    pub const fn sys_time(self) -> u64 {
        self.0
    }
}

impl core::ops::Add<Duration> for SysTime {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(
            self.0
                .saturating_add(u64::try_from(rhs.as_nanos()).unwrap_or(u64::MAX)),
        )
    }
}

impl core::ops::AddAssign<Duration> for SysTime {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl core::ops::Sub<Duration> for SysTime {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(
            self.0
                .saturating_sub(u64::try_from(rhs.as_nanos()).unwrap_or(u64::MAX)),
        )
    }
}

impl core::ops::SubAssign<Duration> for SysTime {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl core::ops::Sub for SysTime {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration::from_nanos(self.0.saturating_sub(rhs.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_nanos() {
        assert_eq!(SysTime::from_nanos(12_345).sys_time(), 12_345);
        assert_eq!(SysTime::ZERO.sys_time(), 0);
        assert_eq!(SysTime::default(), SysTime::ZERO);
    }

    #[test]
    fn arithmetic_saturates_instead_of_panicking() {
        assert_eq!(SysTime::ZERO - Duration::from_secs(1), SysTime::ZERO);
        let mut t = SysTime::ZERO;
        t -= Duration::from_secs(1);
        assert_eq!(t, SysTime::ZERO);
        assert_eq!(SysTime::ZERO - SysTime::from_nanos(1), Duration::ZERO);
        assert_eq!(
            (SysTime::from_nanos(u64::MAX) + Duration::from_secs(1)).sys_time(),
            u64::MAX
        );
        assert_eq!(
            (SysTime::ZERO + Duration::from_secs(u64::MAX)).sys_time(),
            u64::MAX
        );
    }

    #[test]
    fn add_sub_duration() {
        let mut t = SysTime::ZERO + Duration::from_secs(1);
        assert_eq!(t.sys_time(), 1_000_000_000);
        t += Duration::from_secs(2);
        assert_eq!(t.sys_time(), 3_000_000_000);
        t -= Duration::from_secs(1);
        assert_eq!(t.sys_time(), 2_000_000_000);
        assert_eq!((t - Duration::from_secs(2)).sys_time(), 0);
    }

    #[test]
    fn sub_returns_duration() {
        let a = SysTime::ZERO + Duration::from_secs(3);
        let b = SysTime::ZERO + Duration::from_secs(1);
        assert_eq!(a - b, Duration::from_secs(2));
    }
}
