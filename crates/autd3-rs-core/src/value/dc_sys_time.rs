#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DcSysTime(u64);

impl DcSysTime {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_nanos() {
        assert_eq!(DcSysTime::from_nanos(12_345).sys_time(), 12_345);
        assert_eq!(DcSysTime::ZERO.sys_time(), 0);
        assert_eq!(DcSysTime::default(), DcSysTime::ZERO);
    }
}
