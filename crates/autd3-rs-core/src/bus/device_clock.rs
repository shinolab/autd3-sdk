use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use crate::value::SysTime;

const FILTER_WINDOW: u64 = 256;
const FILTER_WINDOW_NS: i64 = 5_000_000_000;

#[derive(Debug, Clone, Default)]
pub struct DeviceClock(Arc<DeviceClockInner>);

#[derive(Debug)]
struct DeviceClockInner {
    origin: Instant,
    offset_ns: AtomicI64,
    prev_max_ns: AtomicI64,
    cur_max_ns: AtomicI64,
    prev_start_ns: AtomicI64,
    cur_start_ns: AtomicI64,
    cur_samples: AtomicU64,
    samples: AtomicU64,
}

impl Default for DeviceClockInner {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            offset_ns: AtomicI64::new(0),
            prev_max_ns: AtomicI64::new(i64::MIN),
            cur_max_ns: AtomicI64::new(i64::MIN),
            prev_start_ns: AtomicI64::new(0),
            cur_start_ns: AtomicI64::new(0),
            cur_samples: AtomicU64::new(0),
            samples: AtomicU64::new(0),
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
        let host_ns = nanos(host);
        let sample_ns = device.sys_time().cast_signed().saturating_sub(host_ns);

        let seen = self.0.samples.load(Ordering::Relaxed);
        let mut cur_max_ns = self.0.cur_max_ns.load(Ordering::Relaxed);
        let mut cur_samples = self.0.cur_samples.load(Ordering::Relaxed);
        let mut prev_max_ns = self.0.prev_max_ns.load(Ordering::Relaxed);
        if seen == 0 {
            self.0.cur_start_ns.store(host_ns, Ordering::Relaxed);
        } else {
            let cur_start_ns = self.0.cur_start_ns.load(Ordering::Relaxed);
            if cur_samples >= FILTER_WINDOW
                || host_ns.saturating_sub(cur_start_ns) >= FILTER_WINDOW_NS
            {
                prev_max_ns = cur_max_ns;
                self.0.prev_start_ns.store(cur_start_ns, Ordering::Relaxed);
                self.0.cur_start_ns.store(host_ns, Ordering::Relaxed);
                cur_max_ns = i64::MIN;
                cur_samples = 0;
            }
        }
        let prev_start_ns = self.0.prev_start_ns.load(Ordering::Relaxed);
        if host_ns.saturating_sub(prev_start_ns) >= 2 * FILTER_WINDOW_NS {
            prev_max_ns = i64::MIN;
        }
        self.0.prev_max_ns.store(prev_max_ns, Ordering::Relaxed);

        cur_max_ns = cur_max_ns.max(sample_ns);
        self.0.cur_max_ns.store(cur_max_ns, Ordering::Relaxed);
        self.0.cur_samples.store(cur_samples + 1, Ordering::Relaxed);

        self.0
            .offset_ns
            .store(cur_max_ns.max(prev_max_ns), Ordering::Relaxed);
        self.0.samples.store(seen + 1, Ordering::Release);
    }

    fn offset_ns(&self) -> Option<i64> {
        (self.0.samples.load(Ordering::Acquire) != 0)
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
    fn a_delayed_sample_does_not_drag_the_offset_down() {
        let clock = DeviceClock::new();
        clock.observe_at(device(1_001_000), HOST);
        for _ in 0..FILTER_WINDOW {
            clock.observe_at(device(1_000_100), HOST);
        }
        assert_eq!(clock.offset_ns(), Some(1_000));
    }

    #[test]
    fn the_window_forgets_a_stale_outlier() {
        let clock = DeviceClock::new();
        clock.observe_at(device(1_001_000), HOST);
        for _ in 0..2 * FILTER_WINDOW {
            clock.observe_at(device(1_000_100), HOST);
        }
        assert_eq!(clock.offset_ns(), Some(100));
    }

    #[test]
    fn the_offset_never_drops_out_of_the_filter_between_windows() {
        let clock = DeviceClock::new();
        for i in 0..4 * FILTER_WINDOW {
            let delayed = i % 3 != 0;
            clock.observe_at(device(if delayed { 1_000_100 } else { 1_001_000 }), HOST);
            assert_eq!(clock.offset_ns(), Some(1_000));
        }
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

    const MS: i64 = 1_000_000;
    const DEVICE_NS_AT_ORIGIN: i64 = 5_000_000_000;
    const DELAY_MIN_NS: i64 = 100_000;
    const DELAY_SPREAD_NS: u64 = 200_000;
    const DELAY_MAX_NS: i64 = 300_000;
    const DRIFT_PPM: [i64; 2] = [93, -93];

    fn true_offset_ns(host_ns: i64, ppm: i64) -> i64 {
        DEVICE_NS_AT_ORIGIN + host_ns * ppm / 1_000_000
    }

    #[derive(Default)]
    struct SampleCountFilter {
        prev_max_ns: Option<i64>,
        cur_max_ns: Option<i64>,
        samples: u64,
    }

    impl SampleCountFilter {
        fn observe(&mut self, sample_ns: i64) -> i64 {
            if self.samples != 0 && self.samples.is_multiple_of(FILTER_WINDOW) {
                self.prev_max_ns = self.cur_max_ns.take();
            }
            self.cur_max_ns = self.cur_max_ns.max(Some(sample_ns));
            self.samples += 1;
            self.cur_max_ns.max(self.prev_max_ns).expect("observed")
        }
    }

    struct DriftingDevice {
        clock: DeviceClock,
        sample_count_filter: SampleCountFilter,
        sample_count_offset_ns: i64,
        host_ns: i64,
        ppm: i64,
        lcg: u64,
    }

    impl DriftingDevice {
        fn new(ppm: i64) -> Self {
            Self {
                clock: DeviceClock::new(),
                sample_count_filter: SampleCountFilter::default(),
                sample_count_offset_ns: 0,
                host_ns: 0,
                ppm,
                lcg: 1,
            }
        }

        fn stay_silent(&mut self, ns: i64) {
            self.host_ns += ns;
        }

        fn reply_after(&mut self, ns: i64) {
            self.host_ns += ns;
            self.lcg = self
                .lcg
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let delay_ns = DELAY_MIN_NS + ((self.lcg >> 33) % DELAY_SPREAD_NS).cast_signed();
            let stamped_host_ns = self.host_ns - delay_ns;
            let device_ns = stamped_host_ns + true_offset_ns(stamped_host_ns, self.ppm);
            self.sample_count_offset_ns =
                self.sample_count_filter.observe(device_ns - self.host_ns);
            self.clock.observe_at(
                device(device_ns.cast_unsigned()),
                Duration::from_nanos(self.host_ns.cast_unsigned()),
            );
        }

        fn error_ns(&self) -> i64 {
            self.clock.offset_ns().expect("observed") - true_offset_ns(self.host_ns, self.ppm)
        }

        fn sample_count_error_ns(&self) -> i64 {
            self.sample_count_offset_ns - true_offset_ns(self.host_ns, self.ppm)
        }
    }

    #[test]
    fn replies_every_10_ms_give_the_same_offset_as_the_sample_count_window() {
        for ppm in DRIFT_PPM {
            let mut unit = DriftingDevice::new(ppm);
            for i in 0..3_000 {
                unit.reply_after(10 * MS + (i % 7) * 100_000);
                assert_eq!(unit.clock.offset_ns(), Some(unit.sample_count_offset_ns));
            }
        }
    }

    #[test]
    fn the_first_reply_after_a_long_silence_replaces_the_maxima_from_before_it() {
        for ppm in DRIFT_PPM {
            for silence_ms in [30_000, 105_000] {
                let mut unit = DriftingDevice::new(ppm);
                for _ in 0..1_000 {
                    unit.reply_after(10 * MS);
                }
                unit.stay_silent(silence_ms * MS);
                let drifted_ns = silence_ms * MS * 93 / 1_000_000;
                assert!(unit.error_ns().abs() > drifted_ns - DELAY_MAX_NS);

                unit.reply_after(10 * MS);
                assert!((-DELAY_MAX_NS..=0).contains(&unit.error_ns()));
                if ppm < 0 {
                    assert!(unit.sample_count_error_ns() > drifted_ns - DELAY_MAX_NS);
                }
            }
        }
    }

    #[test]
    fn sparse_replies_keep_no_maximum_older_than_twice_the_time_window() {
        let bound_ns = 2 * FILTER_WINDOW_NS * 93 / 1_000_000;
        for ppm in DRIFT_PPM {
            for period_ms in [1_000, 10_000] {
                let mut unit = DriftingDevice::new(ppm);
                let mut sample_count_worst_ns = i64::MIN;
                for _ in 0..600 {
                    unit.reply_after(period_ms * MS);
                    assert!((-DELAY_MAX_NS..bound_ns).contains(&unit.error_ns()));
                    sample_count_worst_ns = sample_count_worst_ns.max(unit.sample_count_error_ns());
                }
                if ppm < 0 {
                    assert!(sample_count_worst_ns > 40 * MS);
                }
            }
        }
    }

    #[test]
    fn a_delayed_sample_within_the_time_window_does_not_drag_the_offset_down() {
        const SECOND: Duration = Duration::from_secs(1);
        let clock = DeviceClock::new();
        clock.observe_at(device(1_001_000), Duration::ZERO);
        for i in 1..10 {
            clock.observe_at(device(1_000_100) + SECOND * i, SECOND * i);
            assert_eq!(clock.offset_ns(), Some(1_001_000));
        }
        clock.observe_at(device(1_000_100) + SECOND * 10, SECOND * 10);
        assert_eq!(clock.offset_ns(), Some(1_000_100));
    }
}
