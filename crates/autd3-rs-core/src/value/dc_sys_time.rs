use core::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use thiserror::Error;

const ECAT_EPOCH_OFFSET_NANOS: i128 = 946_684_800 * 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DcSysTimeError {
    #[error("UTC time is out of the representable DcSysTime range (2000-01-01 0:00:00 UTC ..)")]
    OutOfRange,
}

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

    #[must_use]
    pub fn now() -> Self {
        let unix_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is set before the UNIX epoch")
            .as_nanos();
        let nanos = i128::try_from(unix_nanos).expect("current time exceeds i128 range")
            - ECAT_EPOCH_OFFSET_NANOS;
        Self(u64::try_from(nanos).expect("current time exceeds the DcSysTime range"))
    }

    pub fn from_utc(utc: DateTime<Utc>) -> Result<Self, DcSysTimeError> {
        let unix_nanos = utc
            .timestamp_nanos_opt()
            .ok_or(DcSysTimeError::OutOfRange)?;
        let nanos = i128::from(unix_nanos) - ECAT_EPOCH_OFFSET_NANOS;
        u64::try_from(nanos)
            .map(Self)
            .map_err(|_| DcSysTimeError::OutOfRange)
    }

    #[must_use]
    pub fn to_utc(self) -> DateTime<Utc> {
        let unix_nanos = i64::try_from(ECAT_EPOCH_OFFSET_NANOS + i128::from(self.0))
            .expect("DcSysTime exceeds chrono's representable range");
        DateTime::from_timestamp_nanos(unix_nanos)
    }
}

impl core::ops::Add<Duration> for DcSysTime {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + u64::try_from(rhs.as_nanos()).expect("duration exceeds the DcSysTime range"))
    }
}

impl core::ops::AddAssign<Duration> for DcSysTime {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl core::ops::Sub<Duration> for DcSysTime {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(self.0 - u64::try_from(rhs.as_nanos()).expect("duration exceeds the DcSysTime range"))
    }
}

impl core::ops::SubAssign<Duration> for DcSysTime {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl core::ops::Sub for DcSysTime {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration::from_nanos(self.0 - rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::TimeZone;

    #[test]
    fn round_trips_nanos() {
        assert_eq!(DcSysTime::from_nanos(12_345).sys_time(), 12_345);
        assert_eq!(DcSysTime::ZERO.sys_time(), 0);
        assert_eq!(DcSysTime::default(), DcSysTime::ZERO);
    }

    #[test]
    fn now_is_after_epoch() {
        assert!(DcSysTime::now().sys_time() > 0);
    }

    #[test]
    fn from_utc_epoch_is_zero() {
        let epoch = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(DcSysTime::from_utc(epoch).unwrap(), DcSysTime::ZERO);
    }

    #[test]
    fn from_utc_one_second() {
        let t = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 1).unwrap();
        assert_eq!(DcSysTime::from_utc(t).unwrap().sys_time(), 1_000_000_000);
    }

    #[test]
    fn from_utc_one_year() {
        let t = Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            DcSysTime::from_utc(t).unwrap().sys_time(),
            31_622_400_000_000_000
        );
    }

    #[test]
    fn from_utc_before_epoch_is_out_of_range() {
        let t = Utc.with_ymd_and_hms(1999, 1, 1, 0, 0, 1).unwrap();
        assert_eq!(DcSysTime::from_utc(t), Err(DcSysTimeError::OutOfRange));
    }

    #[test]
    fn to_utc_round_trips() {
        let t = Utc.with_ymd_and_hms(2025, 6, 30, 12, 0, 0).unwrap();
        assert_eq!(DcSysTime::from_utc(t).unwrap().to_utc(), t);
    }

    #[test]
    fn add_sub_duration() {
        let mut t = DcSysTime::ZERO + Duration::from_secs(1);
        assert_eq!(t.sys_time(), 1_000_000_000);
        t += Duration::from_secs(2);
        assert_eq!(t.sys_time(), 3_000_000_000);
        t -= Duration::from_secs(1);
        assert_eq!(t.sys_time(), 2_000_000_000);
        assert_eq!((t - Duration::from_secs(2)).sys_time(), 0);
    }

    #[test]
    fn sub_returns_duration() {
        let a = DcSysTime::ZERO + Duration::from_secs(3);
        let b = DcSysTime::ZERO + Duration::from_secs(1);
        assert_eq!(a - b, Duration::from_secs(2));
    }
}
