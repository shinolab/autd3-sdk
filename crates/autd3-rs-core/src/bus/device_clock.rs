use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

use crate::value::{SysTime, SysTimeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClockObservation {
    pub offset_ns: i64,
    pub samples: u64,
}

const FILTER_WINDOW: u64 = 256;

#[derive(Debug, Clone, Default)]
pub struct DeviceClock {
    inner: Arc<DeviceClockInner>,
}

#[derive(Debug)]
struct DeviceClockInner {
    offset_ns: AtomicI64,
    prev_max_ns: AtomicI64,
    cur_max_ns: AtomicI64,
    samples: AtomicU64,
    host_clock_warned: AtomicBool,
}

impl Default for DeviceClockInner {
    fn default() -> Self {
        Self {
            offset_ns: AtomicI64::new(0),
            prev_max_ns: AtomicI64::new(i64::MIN),
            cur_max_ns: AtomicI64::new(i64::MIN),
            samples: AtomicU64::new(0),
            host_clock_warned: AtomicBool::new(false),
        }
    }
}

impl DeviceClock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&self, bus: SysTime) -> Result<(), SysTimeError> {
        match SysTime::now() {
            Ok(host) => {
                self.observe_against(bus, host);
                Ok(())
            }
            Err(e) => {
                if !self.inner.host_clock_warned.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        "the host clock is outside the SysTime range, so the bus offset cannot be observed: {e}"
                    );
                }
                Err(e)
            }
        }
    }

    pub fn observe_against(&self, bus: SysTime, host: SysTime) {
        let sample_ns = bus.sys_time().cast_signed() - host.sys_time().cast_signed();

        let seen = self.inner.samples.load(Ordering::Relaxed);
        let mut cur_max_ns = self.inner.cur_max_ns.load(Ordering::Relaxed);
        if seen != 0 && seen.is_multiple_of(FILTER_WINDOW) {
            self.inner.prev_max_ns.store(cur_max_ns, Ordering::Relaxed);
            cur_max_ns = i64::MIN;
        }
        cur_max_ns = cur_max_ns.max(sample_ns);
        self.inner.cur_max_ns.store(cur_max_ns, Ordering::Relaxed);

        let offset_ns = cur_max_ns.max(self.inner.prev_max_ns.load(Ordering::Relaxed));
        self.inner.offset_ns.store(offset_ns, Ordering::Relaxed);
        self.inner.samples.store(seen + 1, Ordering::Release);
    }

    #[must_use]
    pub fn observation(&self) -> Option<ClockObservation> {
        let samples = self.inner.samples.load(Ordering::Acquire);
        (samples != 0).then(|| ClockObservation {
            offset_ns: self.inner.offset_ns.load(Ordering::Relaxed),
            samples,
        })
    }

    #[must_use]
    pub fn offset_ns(&self) -> Option<i64> {
        self.observation().map(|o| o.offset_ns)
    }

    #[must_use]
    pub fn now(&self) -> Option<SysTime> {
        let offset_ns = self.offset_ns()?;
        let host = SysTime::now().ok()?.sys_time().cast_signed();
        u64::try_from(host + offset_ns)
            .ok()
            .map(SysTime::from_nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unobserved_clock_has_no_offset() {
        let clock = DeviceClock::new();
        assert_eq!(clock.observation(), None);
        assert_eq!(clock.offset_ns(), None);
        assert_eq!(clock.now(), None);
    }

    #[test]
    fn observations_are_visible_through_every_clone() {
        let clock = DeviceClock::new();
        let observer = clock.clone();
        clock.observe_against(
            SysTime::from_nanos(1_000_500),
            SysTime::from_nanos(1_000_000),
        );
        assert_eq!(
            observer.observation(),
            Some(ClockObservation {
                offset_ns: 500,
                samples: 1,
            })
        );

        clock.observe_against(
            SysTime::from_nanos(1_000_900),
            SysTime::from_nanos(1_000_000),
        );
        assert_eq!(
            observer.observation(),
            Some(ClockObservation {
                offset_ns: 900,
                samples: 2,
            })
        );
    }

    #[test]
    fn a_delayed_sample_does_not_drag_the_offset_down() {
        let clock = DeviceClock::new();
        clock.observe_against(
            SysTime::from_nanos(1_001_000),
            SysTime::from_nanos(1_000_000),
        );
        for _ in 0..FILTER_WINDOW {
            clock.observe_against(
                SysTime::from_nanos(1_000_100),
                SysTime::from_nanos(1_000_000),
            );
        }
        assert_eq!(clock.offset_ns(), Some(1_000));
    }

    #[test]
    fn the_window_forgets_a_stale_outlier() {
        let clock = DeviceClock::new();
        clock.observe_against(
            SysTime::from_nanos(1_001_000),
            SysTime::from_nanos(1_000_000),
        );
        for _ in 0..2 * FILTER_WINDOW {
            clock.observe_against(
                SysTime::from_nanos(1_000_100),
                SysTime::from_nanos(1_000_000),
            );
        }
        assert_eq!(clock.offset_ns(), Some(100));
    }

    #[test]
    fn the_offset_never_drops_out_of_the_filter_between_windows() {
        let clock = DeviceClock::new();
        for i in 0..4 * FILTER_WINDOW {
            let delayed = i % 3 != 0;
            clock.observe_against(
                SysTime::from_nanos(if delayed { 1_000_100 } else { 1_001_000 }),
                SysTime::from_nanos(1_000_000),
            );
            assert_eq!(clock.offset_ns(), Some(1_000));
        }
    }

    #[test]
    fn now_applies_the_observed_offset() {
        let clock = DeviceClock::new();
        clock.observe_against(SysTime::from_nanos(500), SysTime::from_nanos(500));
        let before = SysTime::now().unwrap();
        let now = clock.now().expect("observed");
        let after = SysTime::now().unwrap();
        assert!(now >= before && now <= after);
    }
}
