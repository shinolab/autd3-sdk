use std::time::Duration;

use crate::error::{Error, PayloadError};
use crate::protocol::EXEC_AT_UNIT_NS;
use crate::value::SysTime;

const EXEC_LEAD_MIN: Duration = Duration::from_micros(100);
const EXEC_LEAD_LIMIT: Duration = Duration::from_nanos(EXEC_AT_UNIT_NS << 31);
const EXEC_BEHIND_LIMIT: Duration = Duration::from_nanos(EXEC_AT_UNIT_NS << 30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExecAt {
    Immediate,
    Lead(Duration),
    At(SysTime),
}

pub(crate) fn schedule(
    exec: ExecAt,
    lost_timeout: Duration,
    now: impl FnOnce() -> Option<SysTime>,
) -> Result<u32, Error> {
    let limit = lost_timeout.min(EXEC_LEAD_LIMIT);
    match exec {
        ExecAt::Immediate => Ok(0),
        ExecAt::Lead(lead) => {
            if lead < EXEC_LEAD_MIN {
                return Err(PayloadError::ExecLeadTooShort {
                    got: lead,
                    min: EXEC_LEAD_MIN,
                }
                .into());
            }
            if lead >= limit {
                return Err(PayloadError::ExecTooFarAhead { ahead: lead, limit }.into());
            }
            let now = now().ok_or(Error::DeviceTimeUnknown)?;
            Ok(stamp(now + lead))
        }
        ExecAt::At(at) => {
            let now = now().ok_or(Error::DeviceTimeUnknown)?;
            if at >= now {
                let ahead = at - now;
                if ahead >= limit {
                    return Err(PayloadError::ExecTooFarAhead { ahead, limit }.into());
                }
            } else {
                let behind = now - at;
                if behind >= EXEC_BEHIND_LIMIT {
                    return Err(PayloadError::ExecTooFarBehind {
                        behind,
                        limit: EXEC_BEHIND_LIMIT,
                    }
                    .into());
                }
            }
            Ok(stamp(at))
        }
    }
}

#[must_use]
fn stamp(at: SysTime) -> u32 {
    let us = at.sys_time().div_ceil(EXEC_AT_UNIT_NS);
    match u32::try_from(us & u64::from(u32::MAX)) {
        Ok(0) | Err(_) => 1,
        Ok(us) => us,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const US: u64 = EXEC_AT_UNIT_NS;
    const NOW: SysTime = SysTime::from_nanos(20_000 * US);

    fn at_now(exec: ExecAt, lost_timeout: Duration) -> Result<u32, Error> {
        schedule(exec, lost_timeout, || Some(NOW))
    }

    #[test]
    fn stamp_rounds_up_to_the_next_microsecond() {
        assert_eq!(stamp(SysTime::from_nanos(13 * US)), 13);
        assert_eq!(stamp(SysTime::from_nanos(13 * US + 1)), 14);
        assert_eq!(stamp(SysTime::from_nanos(14 * US - 1)), 14);
    }

    #[test]
    fn stamp_keeps_the_low_32_bits_and_never_yields_zero() {
        let wrap = u64::from(u32::MAX) + 1;
        assert_eq!(stamp(SysTime::from_nanos(wrap * US)), 1);
        assert_eq!(stamp(SysTime::from_nanos((wrap + 1) * US)), 1);
        assert_eq!(stamp(SysTime::from_nanos((wrap + 2) * US)), 2);
        assert_eq!(stamp(SysTime::ZERO), 1);
    }

    #[test]
    fn immediate_is_zero_and_does_not_read_the_device_time() {
        assert_eq!(
            schedule(ExecAt::Immediate, Duration::MAX, || unreachable!(
                "the device time is read"
            ))
            .unwrap(),
            0
        );
    }

    #[test]
    fn a_lead_is_added_to_the_device_time() {
        assert_eq!(
            at_now(ExecAt::Lead(Duration::from_millis(5)), Duration::MAX).unwrap(),
            25_000
        );
        assert_eq!(
            at_now(
                ExecAt::Lead(Duration::from_nanos(100 * US + 1)),
                Duration::MAX
            )
            .unwrap(),
            20_101
        );
    }

    #[test]
    fn a_lead_below_100_microseconds_is_rejected_before_reading_the_device_time() {
        assert!(
            schedule(
                ExecAt::Lead(Duration::from_micros(100)),
                Duration::MAX,
                || Some(NOW)
            )
            .is_ok()
        );
        assert!(matches!(
            schedule(
                ExecAt::Lead(Duration::from_nanos(99_999)),
                Duration::MAX,
                || { unreachable!("the device time is read") }
            ),
            Err(Error::InvalidPayload(PayloadError::ExecLeadTooShort { .. }))
        ));
    }

    #[test]
    fn a_lead_not_below_2_pow_31_microseconds_is_rejected() {
        let limit = Duration::from_micros(1 << 31);
        assert!(
            at_now(
                ExecAt::Lead(Duration::from_nanos((1 << 31) * US - 1)),
                Duration::MAX
            )
            .is_ok()
        );
        assert!(matches!(
            at_now(ExecAt::Lead(limit), Duration::MAX),
            Err(Error::InvalidPayload(PayloadError::ExecTooFarAhead { ahead, limit: l }))
                if ahead == limit && l == limit
        ));
    }

    #[test]
    fn a_lead_not_below_the_lost_timeout_is_rejected() {
        let lost_timeout = Duration::from_millis(100);
        assert!(at_now(ExecAt::Lead(Duration::from_micros(99_999)), lost_timeout).is_ok());
        assert!(matches!(
            at_now(ExecAt::Lead(lost_timeout), lost_timeout),
            Err(Error::InvalidPayload(PayloadError::ExecTooFarAhead { limit, .. }))
                if limit == lost_timeout
        ));
    }

    #[test]
    fn an_absolute_time_is_stamped_as_is() {
        assert_eq!(
            at_now(ExecAt::At(SysTime::from_nanos(25_000 * US)), Duration::MAX).unwrap(),
            25_000
        );
        assert_eq!(
            at_now(
                ExecAt::At(SysTime::from_nanos(25_000 * US + 1)),
                Duration::MAX
            )
            .unwrap(),
            25_001
        );
        assert_eq!(at_now(ExecAt::At(NOW), Duration::MAX).unwrap(), 20_000);
    }

    #[test]
    fn an_absolute_time_has_no_lower_bound_on_its_lead() {
        assert_eq!(
            at_now(ExecAt::At(NOW + Duration::from_nanos(1)), Duration::MAX).unwrap(),
            20_001
        );
    }

    #[test]
    fn a_past_absolute_time_within_the_limit_is_accepted() {
        assert_eq!(
            at_now(ExecAt::At(SysTime::from_nanos(19_000 * US)), Duration::MAX).unwrap(),
            19_000
        );
    }

    #[test]
    fn an_absolute_time_too_far_ahead_is_rejected() {
        let limit = Duration::from_micros(1 << 31);
        assert!(
            at_now(
                ExecAt::At(NOW + limit - Duration::from_nanos(1)),
                Duration::MAX
            )
            .is_ok()
        );
        assert!(matches!(
            at_now(ExecAt::At(NOW + limit), Duration::MAX),
            Err(Error::InvalidPayload(PayloadError::ExecTooFarAhead { ahead, .. }))
                if ahead == limit
        ));
        let lost_timeout = Duration::from_millis(100);
        assert!(matches!(
            at_now(ExecAt::At(NOW + lost_timeout), lost_timeout),
            Err(Error::InvalidPayload(PayloadError::ExecTooFarAhead { limit, .. }))
                if limit == lost_timeout
        ));
    }

    #[test]
    fn an_absolute_time_2_pow_30_microseconds_or_more_behind_is_rejected() {
        let limit = Duration::from_micros(1 << 30);
        let now = SysTime::ZERO + limit + Duration::from_secs(1);
        let schedule_at = |at| schedule(ExecAt::At(at), Duration::MAX, || Some(now));
        assert!(schedule_at(now - limit + Duration::from_nanos(1)).is_ok());
        assert!(matches!(
            schedule_at(now - limit),
            Err(Error::InvalidPayload(PayloadError::ExecTooFarBehind { behind, limit: l }))
                if behind == limit && l == limit
        ));
    }

    #[test]
    fn a_lead_or_an_absolute_time_with_an_unknown_device_time_fails() {
        assert!(matches!(
            schedule(
                ExecAt::Lead(Duration::from_millis(5)),
                Duration::MAX,
                || None
            ),
            Err(Error::DeviceTimeUnknown)
        ));
        assert!(matches!(
            schedule(ExecAt::At(NOW), Duration::MAX, || None),
            Err(Error::DeviceTimeUnknown)
        ));
    }
}
