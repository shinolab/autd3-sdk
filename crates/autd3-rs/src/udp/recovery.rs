use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct RecoveryWindow {
    deadline: Option<Instant>,
    awaited_devices: u128,
    recovered_devices: u128,
}

impl RecoveryWindow {
    pub(crate) fn new(now: Instant, window: Duration, awaited_devices: u128) -> Self {
        Self {
            deadline: now.checked_add(window),
            awaited_devices,
            recovered_devices: 0,
        }
    }

    pub(crate) fn restart_with(&mut self, now: Instant, window: Duration, lost_devices: u128) {
        self.deadline = now.checked_add(window);
        self.awaited_devices |= lost_devices;
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub(crate) fn mark_recovered(&mut self, device: usize) {
        self.recovered_devices |= 1 << device;
    }

    pub(crate) fn is_finished(&self, now: Instant) -> bool {
        self.recovered_devices & self.awaited_devices == self.awaited_devices
            || self.deadline.is_some_and(|deadline| now >= deadline)
    }

    pub(crate) fn recovered_devices(&self) -> u128 {
        self.recovered_devices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(1);

    #[test]
    fn nothing_lost_finishes_at_once() {
        let now = Instant::now();
        let window = RecoveryWindow::new(now, WINDOW, 0);
        assert!(window.is_finished(now));
        assert_eq!(window.recovered_devices(), 0);
    }

    #[test]
    fn the_window_closes_when_every_lost_device_is_back() {
        let now = Instant::now();
        let mut window = RecoveryWindow::new(now, WINDOW, 0b1010);
        window.mark_recovered(1);
        assert!(!window.is_finished(now));
        window.mark_recovered(3);
        assert!(window.is_finished(now));
        assert_eq!(window.recovered_devices(), 0b1010);
    }

    #[test]
    fn the_window_closes_at_its_deadline_with_the_devices_back_so_far() {
        let now = Instant::now();
        let mut window = RecoveryWindow::new(now, WINDOW, 0b110);
        window.mark_recovered(2);
        assert_eq!(window.deadline(), Some(now + WINDOW));
        assert!(!window.is_finished(now + WINDOW / 2));
        assert!(window.is_finished(now + WINDOW));
        assert_eq!(window.recovered_devices(), 0b100);
    }

    #[test]
    fn restarting_moves_the_deadline_and_widens_the_awaited_devices() {
        let now = Instant::now();
        let mut window = RecoveryWindow::new(now, WINDOW, 0b01);
        window.mark_recovered(0);
        let later = now + WINDOW / 2;
        window.restart_with(later, WINDOW, 0b10);
        assert_eq!(window.deadline(), Some(later + WINDOW));
        assert!(!window.is_finished(now + WINDOW));
        window.mark_recovered(1);
        assert!(window.is_finished(now + WINDOW));
        assert_eq!(window.recovered_devices(), 0b11);
    }

    #[test]
    fn a_window_too_long_to_represent_waits_for_every_lost_device() {
        let now = Instant::now();
        let mut window = RecoveryWindow::new(now, Duration::MAX, 0b1);
        assert_eq!(window.deadline(), None);
        assert!(!window.is_finished(now));
        window.mark_recovered(0);
        assert!(window.is_finished(now));
    }
}
