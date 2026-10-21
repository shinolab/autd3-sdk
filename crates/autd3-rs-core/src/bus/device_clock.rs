use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Instant;

use crate::value::SysTime;

#[derive(Debug, Clone, Default)]
pub struct DeviceClock(Arc<DeviceClockInner>);

#[derive(Debug)]
struct DeviceClockInner {
    origin: Instant,
    offset_ns: AtomicI64,
    observed: AtomicBool,
}

impl Default for DeviceClockInner {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            offset_ns: AtomicI64::new(0),
            observed: AtomicBool::new(false),
        }
    }
}

fn nanos(host: Duration) -> i64 {
    i64::try_from(host.as_nanos()).unwrap_or(i64::MAX)
}

impl DeviceClock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&self, device: SysTime) {
        self.observe_at(device, self.0.origin.elapsed());
    }

    fn observe_at(&self, device: SysTime, host: Duration) {
        let offset_ns = device.sys_time().cast_signed().saturating_sub(nanos(host));
        self.0.offset_ns.store(offset_ns, Ordering::Relaxed);
        self.0.observed.store(true, Ordering::Release);
    }

    fn offset_ns(&self) -> Option<i64> {
        self.0
            .observed
            .load(Ordering::Acquire)
            .then(|| self.0.offset_ns.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn now(&self) -> Option<SysTime> {
        let offset_ns = self.offset_ns()?;
        let host_ns = nanos(self.0.origin.elapsed());
        u64::try_from(host_ns.saturating_add(offset_ns))
            .ok()
            .map(SysTime::from_nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: Duration = Duration::from_nanos(1_000_000);

    fn device(ns: u64) -> SysTime {
        SysTime::from_nanos(ns)
    }

    #[test]
    fn an_unobserved_clock_has_no_device_time() {
        let clock = DeviceClock::new();
        assert_eq!(clock.offset_ns(), None);
        assert_eq!(clock.now(), None);
    }

    #[test]
    fn observations_are_visible_through_every_clone() {
        let clock = DeviceClock::new();
        let observer = clock.clone();
        clock.observe_at(device(1_000_500), HOST);
        assert_eq!(observer.offset_ns(), Some(500));

        clock.observe_at(device(1_000_900), HOST);
        assert_eq!(observer.offset_ns(), Some(900));
    }

    #[test]
    fn a_device_clock_behind_the_host_clock_has_a_negative_offset() {
        let clock = DeviceClock::new();
        clock.observe_at(SysTime::ZERO, HOST);
        assert_eq!(clock.offset_ns(), Some(-1_000_000));
    }

    #[test]
    fn the_latest_sample_replaces_the_previous_one() {
        let clock = DeviceClock::new();
        clock.observe_at(device(1_001_000), HOST);
        clock.observe_at(device(1_000_100), HOST);
        assert_eq!(clock.offset_ns(), Some(100));
        clock.observe_at(device(1_002_000), HOST);
        assert_eq!(clock.offset_ns(), Some(2_000));
    }

    #[test]
    fn now_runs_on_from_the_observed_device_time() {
        const DEVICE_NS: u64 = 5_000_000_000;
        let clock = DeviceClock::new();
        clock.observe_at(device(DEVICE_NS), Duration::ZERO);
        let now = clock.now().expect("observed");
        let upper = device(DEVICE_NS) + clock.0.origin.elapsed();
        assert!(now >= device(DEVICE_NS) && now <= upper);
    }

    #[test]
    fn observe_reads_the_host_clock_of_this_instance() {
        const DEVICE_NS: u64 = 5_000_000_000;
        let clock = DeviceClock::new();
        clock.observe(device(DEVICE_NS));
        let now = clock.now().expect("observed");
        let upper = device(DEVICE_NS) + clock.0.origin.elapsed();
        assert!(now >= device(DEVICE_NS) && now <= upper);
    }

    #[test]
    fn a_device_time_that_underflows_the_host_clock_is_not_a_time() {
        let clock = DeviceClock::new();
        clock.observe_at(SysTime::ZERO, Duration::from_secs(1 << 40));
        assert_eq!(clock.now(), None);
    }
}
