use core::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use thiserror::Error;

const ECAT_EPOCH_OFFSET_NANOS: i128 = 946_684_800 * 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
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

    fn from_unix_nanos(unix_nanos: i128) -> Result<Self, DcSysTimeError> {
        u64::try_from(unix_nanos - ECAT_EPOCH_OFFSET_NANOS)
            .map(Self)
            .map_err(|_| DcSysTimeError::OutOfRange)
    }

    pub fn now() -> Result<Self, DcSysTimeError> {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DcSysTimeError::OutOfRange)?;
        let unix_nanos = i128::try_from(unix.as_nanos()).map_err(|_| DcSysTimeError::OutOfRange)?;
        Self::from_unix_nanos(unix_nanos)
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
    pub fn with_dc_offset(self, offset_ns: i64) -> Self {
        Self(self.0.saturating_add_signed(offset_ns))
    }

    #[must_use]
    pub fn to_utc(self) -> DateTime<Utc> {
        let unix_nanos =
            i64::try_from(ECAT_EPOCH_OFFSET_NANOS + i128::from(self.0)).unwrap_or(i64::MAX);
        DateTime::from_timestamp_nanos(unix_nanos)
    }
}

impl core::ops::Add<Duration> for DcSysTime {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(
            self.0
                .saturating_add(u64::try_from(rhs.as_nanos()).unwrap_or(u64::MAX)),
        )
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
        Self(
            self.0
                .saturating_sub(u64::try_from(rhs.as_nanos()).unwrap_or(u64::MAX)),
        )
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
        Duration::from_nanos(self.0.saturating_sub(rhs.0))
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
        assert!(DcSysTime::now().unwrap().sys_time() > 0);
    }

    #[test]
    fn a_clock_before_the_ecat_epoch_is_an_error_rather_than_a_panic() {
        for (unix_nanos, what) in [
            (0, "1970-01-01"),
            (-1_000_000_000_000_000_000, "before the UNIX epoch"),
            (
                ECAT_EPOCH_OFFSET_NANOS - 1,
                "one nanosecond short of the epoch",
            ),
        ] {
            assert_eq!(
                DcSysTime::from_unix_nanos(unix_nanos),
                Err(DcSysTimeError::OutOfRange),
                "{what}"
            );
        }
        assert_eq!(
            DcSysTime::from_unix_nanos(ECAT_EPOCH_OFFSET_NANOS),
            Ok(DcSysTime::ZERO)
        );
        assert_eq!(
            DcSysTime::from_unix_nanos(ECAT_EPOCH_OFFSET_NANOS + 1)
                .unwrap()
                .sys_time(),
            1
        );
    }

    #[test]
    fn a_clock_beyond_the_dc_sys_time_range_is_an_error() {
        assert_eq!(
            DcSysTime::from_unix_nanos(i128::MAX),
            Err(DcSysTimeError::OutOfRange)
        );
    }

    #[test]
    fn arithmetic_saturates_instead_of_panicking() {
        assert_eq!(DcSysTime::ZERO - Duration::from_secs(1), DcSysTime::ZERO);
        let mut t = DcSysTime::ZERO;
        t -= Duration::from_secs(1);
        assert_eq!(t, DcSysTime::ZERO);
        assert_eq!(DcSysTime::ZERO - DcSysTime::from_nanos(1), Duration::ZERO);
        assert_eq!(
            (DcSysTime::from_nanos(u64::MAX) + Duration::from_secs(1)).sys_time(),
            u64::MAX
        );
        assert_eq!(
            (DcSysTime::ZERO + Duration::from_secs(u64::MAX)).sys_time(),
            u64::MAX
        );
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
    fn with_dc_offset_shifts_onto_the_bus_clock() {
        let t = DcSysTime::from_nanos(1_000_000);
        assert_eq!(t.with_dc_offset(500).sys_time(), 1_000_500);
        assert_eq!(t.with_dc_offset(-500).sys_time(), 999_500);
        assert_eq!(t.with_dc_offset(0), t);
        assert_eq!(DcSysTime::ZERO.with_dc_offset(-1), DcSysTime::ZERO);
        assert_eq!(
            DcSysTime::from_nanos(u64::MAX).with_dc_offset(1).sys_time(),
            u64::MAX
        );
    }

    #[test]
    fn sub_returns_duration() {
        let a = DcSysTime::ZERO + Duration::from_secs(3);
        let b = DcSysTime::ZERO + Duration::from_secs(1);
        assert_eq!(a - b, Duration::from_secs(2));
    }
}
