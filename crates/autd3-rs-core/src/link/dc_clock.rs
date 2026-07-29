use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::value::DcSysTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcObservation {
    pub offset_ns: i64,
    pub samples: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DcClock {
    inner: Arc<DcClockInner>,
}

#[derive(Debug, Default)]
struct DcClockInner {
    offset_ns: AtomicI64,
    samples: AtomicU64,
}

impl DcClock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&self, bus: DcSysTime) {
        self.observe_against(bus, DcSysTime::now());
    }

    pub fn observe_against(&self, bus: DcSysTime, host: DcSysTime) {
        self.observe_offset(bus.sys_time().cast_signed() - host.sys_time().cast_signed());
    }

    pub fn observe_offset(&self, offset_ns: i64) {
        self.inner.offset_ns.store(offset_ns, Ordering::Relaxed);
        self.inner.samples.fetch_add(1, Ordering::Release);
    }

    #[must_use]
    pub fn observation(&self) -> Option<DcObservation> {
        let samples = self.inner.samples.load(Ordering::Acquire);
        (samples != 0).then(|| DcObservation {
            offset_ns: self.inner.offset_ns.load(Ordering::Relaxed),
            samples,
        })
    }

    #[must_use]
    pub fn offset_ns(&self) -> Option<i64> {
        self.observation().map(|o| o.offset_ns)
    }

    #[must_use]
    pub fn now(&self) -> Option<DcSysTime> {
        let offset_ns = self.offset_ns()?;
        let host = DcSysTime::now().sys_time().cast_signed();
        u64::try_from(host + offset_ns)
            .ok()
            .map(DcSysTime::from_nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unobserved_clock_has_no_offset() {
        let clock = DcClock::new();
        assert_eq!(clock.observation(), None);
        assert_eq!(clock.offset_ns(), None);
        assert_eq!(clock.now(), None);
    }

    #[test]
    fn observations_are_visible_through_every_clone() {
        let clock = DcClock::new();
        let observer = clock.clone();
        clock.observe_against(
            DcSysTime::from_nanos(1_000_500),
            DcSysTime::from_nanos(1_000_000),
        );
        assert_eq!(
            observer.observation(),
            Some(DcObservation {
                offset_ns: 500,
                samples: 1,
            })
        );

        clock.observe_against(
            DcSysTime::from_nanos(1_000_000),
            DcSysTime::from_nanos(1_000_400),
        );
        assert_eq!(
            observer.observation(),
            Some(DcObservation {
                offset_ns: -400,
                samples: 2,
            })
        );
    }

    #[test]
    fn now_applies_the_observed_offset() {
        let clock = DcClock::new();
        clock.observe_against(DcSysTime::from_nanos(500), DcSysTime::from_nanos(500));
        let before = DcSysTime::now();
        let now = clock.now().expect("observed");
        let after = DcSysTime::now();
        assert!(now >= before && now <= after);
    }
}
