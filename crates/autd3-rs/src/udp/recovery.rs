use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct Recovery {
    interval: Duration,
    is_probing: bool,
    next_at: Option<Instant>,
    recovered: u128,
}

impl Recovery {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            is_probing: false,
            next_at: None,
            recovered: 0,
        }
    }

    pub(crate) fn next_at(&self) -> Option<Instant> {
        self.next_at
    }

    pub(crate) fn is_due(&self, now: Instant) -> bool {
        !self.is_probing || self.next_at.is_some_and(|at| now >= at)
    }

    pub(crate) fn idle(&mut self) {
        self.is_probing = false;
        self.next_at = None;
    }

    pub(crate) fn probed(&mut self, now: Instant) -> Option<Instant> {
        self.is_probing = true;
        self.next_at = now.checked_add(self.interval);
        self.next_at
    }

    pub(crate) fn restored(&mut self, unit: usize) {
        self.recovered |= 1 << unit;
    }

    pub(crate) fn take_recovered(&mut self) -> u128 {
        std::mem::take(&mut self.recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recovery() -> Recovery {
        Recovery::new(Duration::from_millis(100))
    }

    #[test]
    fn the_first_probe_is_due_at_once_and_the_next_one_after_the_interval() {
        let mut recovery = recovery();
        let now = Instant::now();
        assert!(recovery.is_due(now));
        let next = recovery.probed(now).unwrap();
        assert_eq!(next, now + Duration::from_millis(100));
        assert_eq!(recovery.next_at(), Some(next));
        assert!(!recovery.is_due(now));
        assert!(recovery.is_due(next));
        recovery.idle();
        assert!(recovery.is_due(now));
        assert_eq!(recovery.next_at(), None);
    }

    #[test]
    fn an_interval_too_long_to_represent_probes_once() {
        let mut recovery = Recovery::new(Duration::MAX);
        let now = Instant::now();
        assert!(recovery.is_due(now));
        assert_eq!(recovery.probed(now), None);
        assert!(!recovery.is_due(now));
    }

    #[test]
    fn the_recovered_units_are_handed_over_once() {
        let mut recovery = recovery();
        recovery.restored(1);
        recovery.restored(3);
        assert_eq!(recovery.take_recovered(), 0b1010);
        assert_eq!(recovery.take_recovered(), 0);
    }
}
