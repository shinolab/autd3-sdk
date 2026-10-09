use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Header;
use autd3_rs_core::FRAME_BYTES_MAX;

const BURST: Duration = Duration::from_millis(4);
const DATAGRAM_OVERHEAD_BYTES: usize = 8 + 40 + 14 + 4 + 8 + 12;

const LINK_BITS_PER_SEC: u32 = 100_000_000;
const MIN_BITS_PER_SEC: u32 = 3_076_000;

pub(crate) const MIN_PERCENT: f32 = 3.076;

const _: () = assert!(
    (size_of::<Header>() + FRAME_BYTES_MAX + DATAGRAM_OVERHEAD_BYTES) as u128 * 8 * 1000
        == MIN_BITS_PER_SEC as u128 * BURST.as_millis()
);

pub(crate) struct Pacer {
    bits_per_sec: u64,
    busy_until: Instant,
}

impl Pacer {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn new(percent: f32) -> Self {
        let bits_per_sec = f64::from(LINK_BITS_PER_SEC) * f64::from(percent) / 100.0;
        Self {
            bits_per_sec: (bits_per_sec.round() as u64).max(1),
            busy_until: Instant::now(),
        }
    }

    const fn wire_time(&self, datagram_bytes: usize) -> Duration {
        let wire_bits = (datagram_bytes + DATAGRAM_OVERHEAD_BYTES) as u64 * 8;
        Duration::from_nanos(wire_bits * 1_000_000_000 / self.bits_per_sec)
    }

    pub(crate) fn reserve(&mut self, now: Instant, datagram_bytes: usize) -> Duration {
        self.busy_until = self.busy_until.max(now) + self.wire_time(datagram_bytes);
        self.busy_until
            .saturating_duration_since(now)
            .saturating_sub(BURST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: usize = 1452;
    const RATE: f32 = 95.0;

    fn wire_time(datagram_bytes: usize) -> Duration {
        Pacer::new(RATE).wire_time(datagram_bytes)
    }

    #[test]
    fn a_datagram_occupies_the_line_for_its_wire_bytes_at_the_capped_rate() {
        assert_eq!(wire_time(FULL), Duration::from_nanos(129_515));
        assert_eq!(wire_time(4), Duration::from_nanos(7_578));
    }

    #[test]
    fn the_wire_time_follows_the_rate() {
        let slowest = Pacer::new(MIN_PERCENT);
        assert_eq!(slowest.bits_per_sec, u64::from(MIN_BITS_PER_SEC));
        assert_eq!(slowest.wire_time(FULL), BURST);
        let saturated = Pacer::new(100.0);
        assert_eq!(saturated.bits_per_sec, u64::from(LINK_BITS_PER_SEC));
        assert!(saturated.wire_time(0) > Duration::ZERO);
    }

    #[test]
    fn a_burst_within_the_allowance_does_not_wait() {
        let mut pacer = Pacer::new(RATE);
        let now = Instant::now();
        let fits = (BURST.as_nanos() / wire_time(FULL).as_nanos()) as usize;
        for _ in 0..fits {
            assert_eq!(pacer.reserve(now, FULL), Duration::ZERO);
        }
        assert!(pacer.reserve(now, FULL) > Duration::ZERO);
    }

    #[test]
    fn a_sender_that_waits_as_told_keeps_the_average_under_the_cap() {
        let mut pacer = Pacer::new(RATE);
        let start = Instant::now();
        let mut now = start;
        let count = 10_000u32;
        for _ in 0..count {
            now += pacer.reserve(now, FULL);
        }
        let elapsed = now - start + BURST;
        assert!(elapsed >= wire_time(FULL) * count);
        assert!(elapsed < wire_time(FULL) * count + wire_time(FULL));
    }

    #[test]
    fn idle_time_is_not_carried_over() {
        let mut pacer = Pacer::new(RATE);
        let start = Instant::now();
        let later = start + Duration::from_secs(10);
        let fits = (BURST.as_nanos() / wire_time(FULL).as_nanos()) as usize;
        for _ in 0..fits {
            assert_eq!(pacer.reserve(later, FULL), Duration::ZERO);
        }
        assert!(pacer.reserve(later, FULL) > Duration::ZERO);
    }

    #[test]
    fn the_wait_is_only_the_excess_over_the_allowance() {
        let mut pacer = Pacer::new(RATE);
        let now = Instant::now();
        let mut wait = Duration::ZERO;
        while wait.is_zero() {
            wait = pacer.reserve(now, FULL);
        }
        assert!(wait <= wire_time(FULL));
    }
}
