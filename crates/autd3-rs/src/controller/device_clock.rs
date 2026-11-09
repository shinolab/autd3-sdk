use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::value::SysTime;

const BUCKET_NS: i128 = 100_000_000;
const WINDOW_NS: i128 = 10_000_000_000;
const MIN_SPAN_FOR_RATE_NS: i128 = 1_000_000_000;
const SENT_TO_GRANDMASTER_RECORDS: usize = 32;

#[derive(Clone, Copy)]
struct OffsetSample {
    host_ns: i128,
    offset_ns: i128,
}

#[derive(Default)]
struct BucketMaxima {
    samples: VecDeque<OffsetSample>,
}

impl BucketMaxima {
    fn insert(&mut self, sample: OffsetSample) {
        let bucket = |s: &OffsetSample| s.host_ns / BUCKET_NS;
        match self.samples.back_mut() {
            Some(last) if bucket(last) == bucket(&sample) => {
                if sample.offset_ns > last.offset_ns {
                    *last = sample;
                }
            }
            Some(last) if bucket(last) > bucket(&sample) => {}
            _ => self.samples.push_back(sample),
        }
    }

    fn forget_older_than_window_before(&mut self, latest_host_ns: i128) {
        while self
            .samples
            .front()
            .is_some_and(|oldest| oldest.host_ns + WINDOW_NS < latest_host_ns)
        {
            self.samples.pop_front();
        }
    }

    fn spans_rate_window(&self) -> bool {
        match (self.samples.front(), self.samples.back()) {
            (Some(first), Some(last)) => last.host_ns - first.host_ns >= MIN_SPAN_FOR_RATE_NS,
            _ => false,
        }
    }

    fn upper_hull(&self) -> Vec<OffsetSample> {
        fn cross(p: OffsetSample, q: OffsetSample, s: OffsetSample) -> i128 {
            (q.host_ns - p.host_ns) * (s.offset_ns - p.offset_ns)
                - (q.offset_ns - p.offset_ns) * (s.host_ns - p.host_ns)
        }

        let mut hull: Vec<OffsetSample> = Vec::with_capacity(self.samples.len());
        for &sample in &self.samples {
            while let [.., p, q] = hull[..]
                && cross(p, q, sample) >= 0
            {
                hull.pop();
            }
            hull.push(sample);
        }
        hull
    }
}

fn centre_of_widest_band(
    upper_hull_of_lower_bounds: &[OffsetSample],
    lower_hull_of_upper_bounds: &[OffsetSample],
    host_ns: i128,
) -> Option<i128> {
    let r_scaled_offset_at_host =
        |rho: i128, r: i128, s: &OffsetSample| s.offset_ns * r + rho * (host_ns - s.host_ns);
    upper_hull_of_lower_bounds
        .array_windows::<2>()
        .chain(lower_hull_of_upper_bounds.array_windows::<2>())
        .filter_map(|[p, q]| {
            let rho = q.offset_ns - p.offset_ns;
            let r = q.host_ns - p.host_ns;
            let c = lower_hull_of_upper_bounds
                .iter()
                .map(|s| r_scaled_offset_at_host(rho, r, s))
                .min()?;
            let f = upper_hull_of_lower_bounds
                .iter()
                .map(|s| r_scaled_offset_at_host(rho, r, s))
                .max()?;
            Some((c, f, r))
        })
        .max_by(|&(c1, f1, r1), &(c2, f2, r2)| ((c1 - f1) * r2).cmp(&((c2 - f2) * r1)))
        .map(|(c, f, r)| (c + f) / (2 * r))
}

fn signed_ns_since(origin: Instant, host: Instant) -> i128 {
    let ns = |d: Duration| i128::try_from(d.as_nanos()).unwrap_or(i128::MAX);
    match host.checked_duration_since(origin) {
        Some(after) => ns(after),
        None => -ns(origin.duration_since(host)),
    }
}

fn negated(samples: Vec<OffsetSample>) -> Vec<OffsetSample> {
    samples
        .into_iter()
        .map(|s| OffsetSample {
            host_ns: s.host_ns,
            offset_ns: -s.offset_ns,
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct DeviceClock {
    origin: Option<Instant>,
    lower_bounds: BucketMaxima,
    negated_upper_bounds: BucketMaxima,
    sent_to_grandmaster: [Option<(u16, Instant)>; SENT_TO_GRANDMASTER_RECORDS],
}

impl DeviceClock {
    fn host_ns(&mut self, host: Instant) -> i128 {
        signed_ns_since(*self.origin.get_or_insert(host), host)
    }

    fn sent_slot(&mut self, msg_id: u16) -> &mut Option<(u16, Instant)> {
        &mut self.sent_to_grandmaster[usize::from(msg_id) % SENT_TO_GRANDMASTER_RECORDS]
    }

    fn forget_samples_outside_window(&mut self) {
        let latest = |bounds: &BucketMaxima| bounds.samples.back().map(|s| s.host_ns);
        if let Some(latest_host_ns) =
            latest(&self.lower_bounds).max(latest(&self.negated_upper_bounds))
        {
            self.lower_bounds
                .forget_older_than_window_before(latest_host_ns);
            self.negated_upper_bounds
                .forget_older_than_window_before(latest_host_ns);
        }
    }

    pub(crate) fn add_lower_bound(&mut self, t3: SysTime, t4: Instant) {
        let host_ns = self.host_ns(t4);
        self.lower_bounds.insert(OffsetSample {
            host_ns,
            offset_ns: i128::from(t3.sys_time()) - host_ns,
        });
        self.forget_samples_outside_window();
    }

    pub(crate) fn record_sent_to_grandmaster(&mut self, msg_id: u16, t1: Instant) {
        self.origin.get_or_insert(t1);
        *self.sent_slot(msg_id) = Some((msg_id, t1));
    }

    pub(crate) fn add_upper_bound_of(&mut self, msg_id: u16, t2: SysTime) {
        let Some((_, t1)) = self.sent_slot(msg_id).filter(|&(id, _)| id == msg_id) else {
            return;
        };
        self.add_upper_bound(t1, t2);
    }

    pub(crate) fn add_upper_bound(&mut self, t1: Instant, t2: SysTime) {
        let host_ns = self.host_ns(t1);
        self.negated_upper_bounds.insert(OffsetSample {
            host_ns,
            offset_ns: host_ns - i128::from(t2.sys_time()),
        });
        self.forget_samples_outside_window();
    }

    fn estimated_offset_at(&self, host_ns: i128) -> Option<i128> {
        let lower = &self.lower_bounds;
        let negated_upper = &self.negated_upper_bounds;
        let (first_lower, last_lower) = (lower.samples.front()?, lower.samples.back()?);
        let (first_upper, last_upper) = (
            negated_upper.samples.front()?,
            negated_upper.samples.back()?,
        );
        let interleaved =
            first_lower.host_ns < last_upper.host_ns && first_upper.host_ns < last_lower.host_ns;
        if lower.spans_rate_window() && negated_upper.spans_rate_window() && interleaved {
            return centre_of_widest_band(
                &lower.upper_hull(),
                &negated(negated_upper.upper_hull()),
                host_ns,
            );
        }
        ((last_lower.host_ns - last_upper.host_ns).abs() < MIN_SPAN_FOR_RATE_NS)
            .then(|| (last_lower.offset_ns - last_upper.offset_ns) / 2)
    }

    pub(crate) fn at(&self, host: Instant) -> Option<SysTime> {
        let host_ns = signed_ns_since(self.origin?, host);
        let device_ns = host_ns + self.estimated_offset_at(host_ns)?;
        Some(SysTime::from_nanos(
            u64::try_from(device_ns.max(0)).unwrap_or(u64::MAX),
        ))
    }

    #[cfg(test)]
    pub(crate) fn upper_bound_buckets(&self) -> usize {
        self.negated_upper_bounds.samples.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELAPSED: Duration = Duration::from_millis(3);
    const FLIGHT: Duration = Duration::from_micros(20);
    const D: u64 = 5_000_000_000;

    fn device(ns: u64) -> SysTime {
        SysTime::from_nanos(ns)
    }

    fn device_at(ms: u64) -> SysTime {
        device(1_000_000_000 + ms * 1_000_000 + ms * 100)
    }

    fn round_trip(
        clock: &mut DeviceClock,
        msg_id: u16,
        sent: Instant,
        device_at_sent: SysTime,
        out: Duration,
        back: Duration,
    ) {
        let received_by_device = device_at_sent + out;
        clock.record_sent_to_grandmaster(msg_id, sent);
        clock.add_lower_bound(received_by_device, sent + out + back);
        clock.add_upper_bound_of(msg_id, received_by_device);
    }

    #[test]
    fn an_unobserved_clock_has_no_device_time() {
        assert_eq!(DeviceClock::default().at(Instant::now()), None);
    }

    #[test]
    fn lower_bounds_alone_give_no_device_time() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.add_lower_bound(device(D), host);
        assert_eq!(clock.at(host), None);
    }

    #[test]
    fn a_round_trip_centres_the_estimate_between_its_bounds() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        round_trip(&mut clock, 1, host, device(D), FLIGHT, FLIGHT);
        assert_eq!(clock.at(host + FLIGHT * 2), Some(device(D) + FLIGHT * 2));
    }

    #[test]
    fn now_runs_on_from_a_round_trip() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        round_trip(&mut clock, 1, host, device(D), FLIGHT, FLIGHT);
        assert_eq!(
            clock.at(host + FLIGHT * 2 + ELAPSED),
            Some(device(D) + FLIGHT * 2 + ELAPSED)
        );
    }

    #[test]
    fn a_host_time_before_the_first_sample_reads_the_device_time_there() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        round_trip(&mut clock, 1, host + ELAPSED, device(D), FLIGHT, FLIGHT);
        assert_eq!(clock.at(host), Some(device(D) - ELAPSED));
    }

    #[test]
    fn a_request_or_reply_delayed_on_the_way_does_not_move_the_estimate() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        round_trip(&mut clock, 1, host, device(D), FLIGHT, FLIGHT);
        round_trip(
            &mut clock,
            2,
            host + ELAPSED,
            device(D) + ELAPSED,
            FLIGHT * 5,
            FLIGHT * 5,
        );
        assert_eq!(
            clock.at(host + ELAPSED + FLIGHT * 10),
            Some(device(D) + ELAPSED + FLIGHT * 10)
        );
    }

    #[test]
    fn the_rate_is_taken_from_both_bounds_past_delayed_round_trips() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        for (msg_id, ms) in (0..3_000).step_by(30).enumerate() {
            let delay = Duration::from_micros(if ms % 90 == 0 { 0 } else { 30 + ms % 7 });
            let (out, back) = if ms % 60 == 0 {
                (FLIGHT + delay, FLIGHT)
            } else {
                (FLIGHT, FLIGHT + delay)
            };
            round_trip(
                &mut clock,
                u16::try_from(msg_id).unwrap(),
                host + Duration::from_millis(ms),
                device_at(ms),
                out,
                back,
            );
        }
        let estimate = clock.at(host + Duration::from_secs(60)).unwrap();
        let error = estimate.sys_time().abs_diff(device_at(60_000).sys_time());
        assert!(error < 1_000, "{error} ns off");
    }

    #[test]
    fn an_asymmetric_round_trip_leaves_half_the_difference() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        for (msg_id, ms) in (0..3_000).step_by(30).enumerate() {
            round_trip(
                &mut clock,
                u16::try_from(msg_id).unwrap(),
                host + Duration::from_millis(ms),
                device_at(ms),
                FLIGHT / 2,
                FLIGHT * 3 / 2,
            );
        }
        let estimate = clock.at(host + Duration::from_secs(3)).unwrap();
        let behind = device_at(3_000).sys_time() - estimate.sys_time();
        assert!(behind.abs_diff(10_000) < 1_000, "{behind} ns behind");
    }

    #[test]
    fn samples_older_than_ten_seconds_are_forgotten() {
        let host = Instant::now();
        let later = host + Duration::from_secs(11);
        let mut clock = DeviceClock::default();
        round_trip(&mut clock, 1, host, device(D), FLIGHT, FLIGHT);
        round_trip(&mut clock, 2, later, device(D), FLIGHT, FLIGHT);
        assert_eq!(clock.at(later + FLIGHT * 2), Some(device(D) + FLIGHT * 2));
    }

    #[test]
    fn upper_bounds_that_stop_arriving_leave_no_device_time() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        for (msg_id, ms) in (0..14_000).step_by(30).enumerate() {
            let sent = host + Duration::from_millis(ms);
            if ms < 3_000 {
                round_trip(
                    &mut clock,
                    u16::try_from(msg_id % 1_000).unwrap(),
                    sent,
                    device_at(ms),
                    FLIGHT,
                    FLIGHT,
                );
            } else {
                clock.add_lower_bound(device_at(ms) + FLIGHT, sent + FLIGHT * 2);
            }
        }
        assert_eq!(clock.at(host + Duration::from_secs(14)), None);
    }

    #[test]
    fn a_reply_without_its_send_recorded_gives_no_upper_bound() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.add_lower_bound(device(D) + FLIGHT, host + FLIGHT * 2);
        clock.add_upper_bound_of(1, device(D) + FLIGHT);
        assert_eq!(clock.at(host + FLIGHT * 2), None);
    }

    #[test]
    fn a_send_record_reused_by_a_later_msg_id_is_not_paired() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.record_sent_to_grandmaster(1, host);
        clock.record_sent_to_grandmaster(
            1 + u16::try_from(SENT_TO_GRANDMASTER_RECORDS).unwrap(),
            host + ELAPSED,
        );
        clock.add_lower_bound(device(D) + FLIGHT, host + FLIGHT * 2);
        clock.add_upper_bound_of(1, device(D) + FLIGHT);
        assert_eq!(clock.at(host + FLIGHT * 2), None);
    }

    #[test]
    fn a_time_set_at_open_bounds_the_offset_from_both_sides() {
        let host = Instant::now();
        let mut clock = DeviceClock::default();
        clock.add_upper_bound(host, device(D));
        clock.add_lower_bound(device(D), host + FLIGHT * 2);
        assert_eq!(clock.at(host + FLIGHT * 2), Some(device(D) + FLIGHT));
    }
}
