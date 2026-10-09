use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
struct Counters {
    frames: AtomicU64,
    resets: AtomicU64,
    heartbeats: AtomicU64,
    missed_replies: AtomicU64,
    acked_frames: AtomicU64,
    ack_latency_ns_total: AtomicU64,
    worst_ack_latency_ns: AtomicU64,
}

#[derive(Debug, Clone, Default)]
pub struct BusStats(Arc<Counters>);

impl BusStats {
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.0.frames.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn resets(&self) -> u64 {
        self.0.resets.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn heartbeats(&self) -> u64 {
        self.0.heartbeats.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn missed_replies(&self) -> u64 {
        self.0.missed_replies.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn acked_frames(&self) -> u64 {
        self.0.acked_frames.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn worst_ack_latency_ns(&self) -> u64 {
        self.0.worst_ack_latency_ns.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn mean_ack_latency_ns(&self) -> u64 {
        let total = self.0.ack_latency_ns_total.load(Ordering::Acquire);
        let acked = self.0.acked_frames.load(Ordering::Acquire);
        if acked == 0 {
            return 0;
        }
        total / acked
    }

    pub fn record_frame(&self) {
        self.0.frames.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_reset(&self) {
        self.0.resets.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_heartbeat(&self) {
        self.0.heartbeats.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_missed_replies(&self, count: u64) {
        self.0.missed_replies.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_ack(&self, latency_ns: u64) {
        self.0
            .worst_ack_latency_ns
            .fetch_max(latency_ns, Ordering::Relaxed);
        self.0.acked_frames.fetch_add(1, Ordering::Relaxed);
        self.0
            .ack_latency_ns_total
            .fetch_add(latency_ns, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn counters_are_shared_between_clones() {
        let stats = BusStats::default();
        let observer = stats.clone();
        stats.record_frame();
        stats.record_frame();
        stats.record_reset();
        stats.record_heartbeat();
        stats.record_missed_replies(2);
        assert_eq!(observer.frames(), 2);
        assert_eq!(observer.resets(), 1);
        assert_eq!(observer.heartbeats(), 1);
        assert_eq!(observer.missed_replies(), 2);
    }

    #[test]
    fn ack_latencies_keep_the_mean_and_the_worst() {
        let stats = BusStats::default();
        let observer = stats.clone();
        assert_eq!(observer.mean_ack_latency_ns(), 0, "no division by zero");
        stats.record_ack(100_000);
        stats.record_ack(300_000);
        stats.record_ack(200_000);
        assert_eq!(observer.acked_frames(), 3);
        assert_eq!(observer.mean_ack_latency_ns(), 200_000);
        assert_eq!(observer.worst_ack_latency_ns(), 300_000);
    }

    #[test]
    fn the_mean_never_exceeds_the_worst_sample_while_a_writer_is_running() {
        const SAMPLE_NS: u64 = 1_000_000_000;

        let stats = BusStats::default();
        let observer = stats.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let writer_stop = stop.clone();
        let writer = std::thread::spawn(move || {
            while !writer_stop.load(Ordering::Acquire) {
                stats.record_ack(SAMPLE_NS);
            }
        });
        for _ in 0..100_000 {
            let mean = observer.mean_ack_latency_ns();
            assert!(
                mean <= SAMPLE_NS,
                "the mean must stay within the samples, got {mean}"
            );
        }
        stop.store(true, Ordering::Release);
        writer.join().unwrap();
    }
}
