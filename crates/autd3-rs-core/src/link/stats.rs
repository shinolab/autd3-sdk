use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Default)]
pub struct LinkStats {
    stale_cycles: Arc<AtomicU64>,
    lost_cycles: Arc<AtomicU64>,
    phase_excursions: Arc<AtomicU64>,
    worst_phase_deviation_ns: Arc<AtomicU64>,
    exchanges: Arc<AtomicU64>,
    exchange_ns_total: Arc<AtomicU64>,
    worst_exchange_ns: Arc<AtomicU64>,
}

impl LinkStats {
    #[must_use]
    pub fn stale_cycles(&self) -> u64 {
        self.stale_cycles.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn lost_cycles(&self) -> u64 {
        self.lost_cycles.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn phase_excursions(&self) -> u64 {
        self.phase_excursions.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn worst_phase_deviation_ns(&self) -> u64 {
        self.worst_phase_deviation_ns.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn exchanges(&self) -> u64 {
        self.exchanges.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn worst_exchange_ns(&self) -> u64 {
        self.worst_exchange_ns.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn mean_exchange_ns(&self) -> u64 {
        let exchanges = self.exchanges();
        if exchanges == 0 {
            return 0;
        }
        self.exchange_ns_total.load(Ordering::Acquire) / exchanges
    }

    pub fn record_exchange(&self, elapsed_ns: u64) {
        self.exchanges.fetch_add(1, Ordering::Relaxed);
        self.exchange_ns_total
            .fetch_add(elapsed_ns, Ordering::Relaxed);
        self.worst_exchange_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    pub fn record_stale_cycle(&self) {
        self.stale_cycles.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_lost_cycle(&self) {
        self.lost_cycles.fetch_add(1, Ordering::Relaxed);
        self.stale_cycles.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_phase_excursion(&self, deviation_ns: u64) {
        self.phase_excursions.fetch_add(1, Ordering::Relaxed);
        self.worst_phase_deviation_ns
            .fetch_max(deviation_ns, Ordering::Relaxed);
    }

    pub fn add_stale_cycles(&self, count: u64) {
        self.stale_cycles.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_lost_cycles(&self, count: u64) {
        self.lost_cycles.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_phase_excursions(&self, count: u64, worst_deviation_ns: u64) {
        self.phase_excursions.fetch_add(count, Ordering::Relaxed);
        self.worst_phase_deviation_ns
            .fetch_max(worst_deviation_ns, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_stats_counters() {
        let stats = LinkStats::default();
        let observer = stats.clone();
        stats.record_stale_cycle();
        stats.record_lost_cycle();
        assert_eq!(observer.stale_cycles(), 2);
        assert_eq!(observer.lost_cycles(), 1);
    }

    #[test]
    fn exchange_times_keep_the_mean_and_the_worst() {
        let stats = LinkStats::default();
        let observer = stats.clone();
        assert_eq!(observer.mean_exchange_ns(), 0, "no division by zero");
        stats.record_exchange(100_000);
        stats.record_exchange(300_000);
        stats.record_exchange(200_000);
        assert_eq!(observer.exchanges(), 3);
        assert_eq!(observer.mean_exchange_ns(), 200_000);
        assert_eq!(observer.worst_exchange_ns(), 300_000);
    }

    #[test]
    fn phase_excursions_keep_the_worst_deviation() {
        let stats = LinkStats::default();
        let observer = stats.clone();
        stats.record_phase_excursion(1_000);
        stats.record_phase_excursion(300);
        stats.record_phase_excursion(2_500);
        assert_eq!(observer.phase_excursions(), 3);
        assert_eq!(observer.worst_phase_deviation_ns(), 2_500);
    }

    #[test]
    fn counters_can_be_advanced_in_bulk() {
        let stats = LinkStats::default();
        stats.add_stale_cycles(5);
        stats.add_lost_cycles(2);
        stats.add_phase_excursions(7, 900);
        stats.add_phase_excursions(1, 100);
        assert_eq!(stats.stale_cycles(), 5);
        assert_eq!(stats.lost_cycles(), 2);
        assert_eq!(stats.phase_excursions(), 8);
        assert_eq!(stats.worst_phase_deviation_ns(), 900);
    }
}
