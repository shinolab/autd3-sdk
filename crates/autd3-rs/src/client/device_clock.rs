use std::collections::VecDeque;
use std::time::Instant;

use crate::value::SysTime;

const BUCKET_NS: i128 = 100_000_000;
const WINDOW_NS: i128 = 10_000_000_000;
const MIN_SPAN_FOR_RATE_NS: i128 = 1_000_000_000;

#[derive(Clone, Copy)]
struct OffsetSample {
    host_ns: i128,
    offset_ns: i128,
}

#[derive(Default)]
pub(crate) struct DeviceClock {
    origin: Option<Instant>,
    bucket_maxima: VecDeque<OffsetSample>,
}

fn cross(o: OffsetSample, a: OffsetSample, b: OffsetSample) -> i128 {
    (a.host_ns - o.host_ns) * (b.offset_ns - o.offset_ns)
        - (a.offset_ns - o.offset_ns) * (b.host_ns - o.host_ns)
}

impl DeviceClock {
    fn host_ns(origin: Instant, host: Instant) -> i128 {
        i128::try_from(host.saturating_duration_since(origin).as_nanos()).unwrap_or(i128::MAX)
    }

    pub(crate) fn observe(&mut self, device: SysTime, host: Instant) {
        let host_ns = Self::host_ns(*self.origin.get_or_insert(host), host);
        let sample = OffsetSample {
            host_ns,
            offset_ns: i128::from(device.sys_time()) - host_ns,
        };
        match self.bucket_maxima.back_mut() {
            Some(last) if last.host_ns / BUCKET_NS >= host_ns / BUCKET_NS => {
                if sample.offset_ns > last.offset_ns {
                    *last = sample;
                }
            }
            _ => self.bucket_maxima.push_back(sample),
        }
        while self
            .bucket_maxima
            .front()
            .is_some_and(|oldest| oldest.host_ns + WINDOW_NS < host_ns)
        {
            self.bucket_maxima.pop_front();
        }
    }

    fn offset_at(&self, host_ns: i128) -> Option<i128> {
        let (&first, &last) = (self.bucket_maxima.front()?, self.bucket_maxima.back()?);
        if last.host_ns - first.host_ns < MIN_SPAN_FOR_RATE_NS {
            return Some(last.offset_ns);
        }
        let mut upper_hull: Vec<OffsetSample> = Vec::with_capacity(self.bucket_maxima.len());
        for &sample in &self.bucket_maxima {
            while let [.., o, a] = upper_hull[..]
                && cross(o, a, sample) >= 0
            {
                upper_hull.pop();
            }
            upper_hull.push(sample);
        }
        let mean_host_ns = self.bucket_maxima.iter().map(|s| s.host_ns).sum::<i128>()
            / i128::try_from(self.bucket_maxima.len()).ok()?;
        let [a, b] = *upper_hull
            .array_windows::<2>()
            .find(|[_, b]| b.host_ns >= mean_host_ns)?;
        Some(
            a.offset_ns
                + (b.offset_ns - a.offset_ns) * (host_ns - a.host_ns) / (b.host_ns - a.host_ns),
        )
    }

    pub(crate) fn now(&self, host: Instant) -> Option<SysTime> {
        let host_ns = Self::host_ns(self.origin?, host);
        let device_ns = host_ns + self.offset_at(host_ns)?;
        Some(SysTime::from_nanos(
            u64::try_from(device_ns.max(0)).unwrap_or(u64::MAX),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const ELAPSED: Duration = Duration::from_millis(3);

    fn device(ns: u64) -> SysTime {
        SysTime::from_nanos(ns)
    }

    #[test]
    fn an_unobserved_clock_has_no_device_time() {
        assert_eq!(DeviceClock::default().now(Instant::now()), None);
    }

    #[test]
    fn now_runs_on_from_the_observed_device_time() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(5_000_000_000), host);
        assert_eq!(clock.now(host), Some(device(5_000_000_000)));
        assert_eq!(
            clock.now(host + ELAPSED),
            Some(device(5_000_000_000) + ELAPSED)
        );
    }

    #[test]
    fn a_reply_read_late_does_not_pull_the_estimate_back() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(5_000_000_000), host);
        clock.observe(device(5_000_000_000), host + ELAPSED);
        assert_eq!(
            clock.now(host + ELAPSED),
            Some(device(5_000_000_000) + ELAPSED)
        );
    }

    #[test]
    fn a_host_time_before_the_sample_reads_the_sampled_device_time() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(1_000), host + ELAPSED);
        assert_eq!(clock.now(host), Some(device(1_000)));
    }

    #[test]
    fn the_rate_of_the_device_is_extrapolated_past_delayed_samples() {
        let host = Instant::now();
        let device_at = |ms: u64| device(1_000_000_000 + ms * 1_000_000 + ms * 100);
        let mut clock = DeviceClock::default();
        for ms in (0..3_000).step_by(30) {
            let read_delay = Duration::from_micros(if ms % 90 == 0 { 0 } else { 20 + ms % 7 });
            clock.observe(device_at(ms), host + Duration::from_millis(ms) + read_delay);
        }
        assert_eq!(
            clock.now(host + Duration::from_millis(60_000)),
            Some(device_at(60_000))
        );
    }

    #[test]
    fn samples_older_than_ten_seconds_are_forgotten() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(1_000_000_000), host);
        clock.observe(device(1_000_000_000), host + Duration::from_secs(11));
        assert_eq!(
            clock.now(host + Duration::from_secs(11)),
            Some(device(1_000_000_000))
        );
    }
}
