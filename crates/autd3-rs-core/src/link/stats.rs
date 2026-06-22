use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Default)]
pub struct LinkStats {
    stale_cycles: Arc<AtomicU64>,
    lost_cycles: Arc<AtomicU64>,
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

    pub fn record_stale_cycle(&self) {
        self.stale_cycles.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_lost_cycle(&self) {
        self.lost_cycles.fetch_add(1, Ordering::Relaxed);
        self.stale_cycles.fetch_add(1, Ordering::Relaxed);
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
}
