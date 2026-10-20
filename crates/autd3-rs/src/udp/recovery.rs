use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct Recovery {
    until: Option<Instant>,
    lost: u128,
    recovered: u128,
}

impl Recovery {
    pub(crate) fn new(now: Instant, window: Duration, lost: u128) -> Self {
        Self {
            until: now.checked_add(window),
            lost,
            recovered: 0,
        }
    }

    pub(crate) fn extend(&mut self, now: Instant, window: Duration, lost: u128) {
        self.until = now.checked_add(window);
        self.lost |= lost;
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.until
    }

    pub(crate) fn restored(&mut self, unit: usize) {
        self.recovered |= 1 << unit;
    }

    pub(crate) fn is_settled(&self, now: Instant) -> bool {
        self.recovered & self.lost == self.lost || self.until.is_some_and(|until| now >= until)
    }

    pub(crate) fn recovered(&self) -> u128 {
        self.recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(1);

    #[test]
    fn nothing_lost_settles_at_once() {
        let now = Instant::now();
        let recovery = Recovery::new(now, WINDOW, 0);
        assert!(recovery.is_settled(now));
        assert_eq!(recovery.recovered(), 0);
    }

    #[test]
    fn the_window_closes_when_every_lost_unit_is_back() {
        let now = Instant::now();
        let mut recovery = Recovery::new(now, WINDOW, 0b1010);
        recovery.restored(1);
        assert!(!recovery.is_settled(now));
        recovery.restored(3);
        assert!(recovery.is_settled(now));
        assert_eq!(recovery.recovered(), 0b1010);
    }

    #[test]
    fn the_window_closes_at_its_deadline_with_the_units_back_so_far() {
        let now = Instant::now();
        let mut recovery = Recovery::new(now, WINDOW, 0b110);
        recovery.restored(2);
        assert_eq!(recovery.deadline(), Some(now + WINDOW));
        assert!(!recovery.is_settled(now + WINDOW / 2));
        assert!(recovery.is_settled(now + WINDOW));
        assert_eq!(recovery.recovered(), 0b100);
    }

    #[test]
    fn extending_moves_the_deadline_and_widens_the_lost_units() {
        let now = Instant::now();
        let mut recovery = Recovery::new(now, WINDOW, 0b01);
        recovery.restored(0);
        let later = now + WINDOW / 2;
        recovery.extend(later, WINDOW, 0b10);
        assert_eq!(recovery.deadline(), Some(later + WINDOW));
        assert!(!recovery.is_settled(now + WINDOW));
        recovery.restored(1);
        assert!(recovery.is_settled(now + WINDOW));
        assert_eq!(recovery.recovered(), 0b11);
    }

    #[test]
    fn a_window_too_long_to_represent_waits_for_every_lost_unit() {
        let now = Instant::now();
        let mut recovery = Recovery::new(now, Duration::MAX, 0b1);
        assert_eq!(recovery.deadline(), None);
        assert!(!recovery.is_settled(now));
        recovery.restored(0);
        assert!(recovery.is_settled(now));
    }
}
