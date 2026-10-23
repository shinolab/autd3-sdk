use std::time::Instant;

use crate::value::SysTime;

#[derive(Clone, Copy, Debug)]
struct DeviceTimeSample {
    host: Instant,
    device: SysTime,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DeviceClock {
    last_sample: Option<DeviceTimeSample>,
}

impl DeviceClock {
    pub(crate) fn observe(&mut self, device: SysTime, host: Instant) {
        self.last_sample = Some(DeviceTimeSample { host, device });
    }

    pub(crate) fn now(&self, host: Instant) -> Option<SysTime> {
        self.last_sample
            .map(|sample| sample.device + host.saturating_duration_since(sample.host))
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
    fn the_latest_sample_replaces_the_previous_one() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(1_000_000), host);
        clock.observe(device(500), host + ELAPSED);
        assert_eq!(clock.now(host + ELAPSED * 2), Some(device(500) + ELAPSED));
    }

    #[test]
    fn a_host_time_before_the_sample_reads_the_sampled_device_time() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.observe(device(1_000), host + ELAPSED);
        assert_eq!(clock.now(host), Some(device(1_000)));
    }
}
