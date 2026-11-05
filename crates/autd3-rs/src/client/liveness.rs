use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::DeviceState;

#[derive(Clone, Copy)]
pub(crate) struct DeviceLiveness {
    unanswered_since: Option<Instant>,
    answered_at: Option<Instant>,
    send_failing_since: Option<Instant>,
    last_flags: Flags,
}

impl DeviceLiveness {
    pub(crate) fn new() -> Self {
        Self {
            unanswered_since: None,
            answered_at: None,
            send_failing_since: None,
            last_flags: Flags::PTP_LOCKED | Flags::SYNC_READY,
        }
    }

    pub(crate) fn unsynchronized() -> Self {
        Self {
            last_flags: Flags::empty(),
            ..Self::new()
        }
    }

    pub(crate) fn record_send_failure(&mut self, now: Instant, lost_timeout: Duration) -> bool {
        let since = *self.send_failing_since.get_or_insert(now);
        now.saturating_duration_since(since) >= lost_timeout
    }

    pub(crate) fn start_unanswered_clock(&mut self, requested_at: Instant) -> bool {
        if self
            .answered_at
            .is_some_and(|answered_at| answered_at >= requested_at)
        {
            return false;
        }
        let starts_afresh = self.unanswered_since.is_none();
        self.unanswered_since.get_or_insert(requested_at);
        starts_afresh
    }

    pub(crate) fn accept_reply(
        &mut self,
        expected_unit_id: usize,
        reported_unit_id: usize,
        flags: Flags,
        now: Instant,
    ) -> bool {
        if !flags.contains(Flags::ASSIGNED) || reported_unit_id != expected_unit_id {
            return false;
        }
        self.unanswered_since = None;
        self.answered_at = Some(now);
        self.send_failing_since = None;
        self.last_flags = flags;
        true
    }

    pub(crate) fn is_unanswered_past(self, now: Instant, lost_timeout: Duration) -> bool {
        self.unanswered_since
            .is_some_and(|since| now.saturating_duration_since(since) >= lost_timeout)
    }

    pub(crate) fn lost_deadline(self, lost_timeout: Duration) -> Option<Instant> {
        self.unanswered_since
            .and_then(|since| since.checked_add(lost_timeout))
    }

    pub(crate) fn state(self) -> DeviceState {
        if self
            .last_flags
            .contains(Flags::PTP_LOCKED | Flags::SYNC_READY)
        {
            DeviceState::Ready
        } else {
            DeviceState::Syncing
        }
    }

    pub(crate) fn reported_flags(self) -> Flags {
        self.last_flags & (Flags::PTP_LOCKED | Flags::FAILSAFE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Flags = Flags::ASSIGNED
        .union(Flags::PTP_LOCKED)
        .union(Flags::SYNC_READY);
    const LOST_AFTER: Duration = Duration::from_millis(100);

    #[test]
    fn only_the_first_unanswered_request_starts_the_clock() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert!(liveness.start_unanswered_clock(now));
        assert!(!liveness.start_unanswered_clock(now + LOST_AFTER / 2));
        assert!(liveness.accept_reply(1, 1, READY, now + LOST_AFTER / 2));
        assert!(liveness.start_unanswered_clock(now + LOST_AFTER));
    }

    #[test]
    fn a_reply_read_before_its_request_is_recorded_starts_no_clock() {
        let mut liveness = DeviceLiveness::new();
        let requested_at = Instant::now();
        assert!(liveness.accept_reply(1, 1, READY, requested_at + Duration::from_micros(1)));
        assert!(!liveness.start_unanswered_clock(requested_at));
        assert!(!liveness.is_unanswered_past(requested_at + LOST_AFTER * 2, LOST_AFTER));
    }

    #[test]
    fn a_reply_older_than_the_request_does_not_stop_its_clock() {
        let mut liveness = DeviceLiveness::new();
        let answered_at = Instant::now();
        let requested_at = answered_at + Duration::from_micros(1);
        assert!(liveness.accept_reply(1, 1, READY, answered_at));
        assert!(liveness.start_unanswered_clock(requested_at));
        assert!(liveness.is_unanswered_past(requested_at + LOST_AFTER, LOST_AFTER));
    }

    #[test]
    fn a_send_that_fails_once_is_not_a_loss() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert!(!liveness.record_send_failure(now, LOST_AFTER));
        assert!(!liveness.is_unanswered_past(now + LOST_AFTER * 2, LOST_AFTER));
    }

    #[test]
    fn sends_that_keep_failing_past_the_lost_timeout_are_a_loss() {
        let mut liveness = DeviceLiveness::new();
        let start = Instant::now();
        assert!(!liveness.record_send_failure(start, LOST_AFTER));
        assert!(!liveness.record_send_failure(start + LOST_AFTER / 2, LOST_AFTER));
        assert!(liveness.record_send_failure(start + LOST_AFTER, LOST_AFTER));
    }

    #[test]
    fn a_reply_between_two_failed_sends_restarts_the_count() {
        let mut liveness = DeviceLiveness::new();
        let start = Instant::now();
        assert!(!liveness.record_send_failure(start, LOST_AFTER));
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 2));
        assert!(!liveness.record_send_failure(start + LOST_AFTER, LOST_AFTER));
        assert!(liveness.record_send_failure(start + LOST_AFTER * 2, LOST_AFTER));
    }

    #[test]
    fn the_reported_flags_follow_the_last_reply() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert_eq!(liveness.reported_flags(), Flags::PTP_LOCKED);

        assert!(liveness.accept_reply(1, 1, READY | Flags::FAILSAFE, now));
        assert_eq!(
            liveness.reported_flags(),
            Flags::PTP_LOCKED | Flags::FAILSAFE
        );

        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::FAILSAFE, now));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert_eq!(liveness.reported_flags(), Flags::FAILSAFE);

        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::SYNC_READY, now));
        assert_eq!(liveness.reported_flags(), Flags::empty());
    }

    #[test]
    fn an_unsynchronized_device_is_syncing_until_its_first_reply() {
        let mut liveness = DeviceLiveness::unsynchronized();
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert_eq!(liveness.reported_flags(), Flags::empty());

        assert!(liveness.accept_reply(1, 1, READY, Instant::now()));
        assert_eq!(liveness.state(), DeviceState::Ready);
        assert_eq!(liveness.reported_flags(), Flags::PTP_LOCKED);
    }

    #[test]
    fn a_ready_reply_is_ready() {
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(2, 2, READY, Instant::now()));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_pulse_is_syncing() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert!(liveness.accept_reply(0, 0, Flags::ASSIGNED, now));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(0, 0, READY, now));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_ptp_lock_is_syncing() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::SYNC_READY, now));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::PTP_LOCKED, now));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(1, 1, READY, now));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unanswered_request_is_a_loss_once_the_lost_timeout_passes_since_the_first_one() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        liveness.start_unanswered_clock(start);
        assert!(!liveness.is_unanswered_past(start + LOST_AFTER / 2, LOST_AFTER));
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 4));
        let later = start + LOST_AFTER / 2;
        liveness.start_unanswered_clock(later);
        liveness.start_unanswered_clock(later + LOST_AFTER / 2);
        assert!(!liveness.is_unanswered_past(start + LOST_AFTER, LOST_AFTER));
        assert!(liveness.is_unanswered_past(later + LOST_AFTER, LOST_AFTER));
    }

    #[test]
    fn the_lost_deadline_follows_the_first_unanswered_request() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
        liveness.start_unanswered_clock(start);
        liveness.start_unanswered_clock(start + LOST_AFTER / 2);
        assert_eq!(liveness.lost_deadline(LOST_AFTER), Some(start + LOST_AFTER));
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 2));
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
    }

    #[test]
    fn silence_without_a_request_is_not_a_loss() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(1, 1, READY, start));
        assert!(!liveness.is_unanswered_past(start + LOST_AFTER * 10, LOST_AFTER));
        liveness.start_unanswered_clock(start + LOST_AFTER * 10);
        assert!(!liveness.is_unanswered_past(start + LOST_AFTER * 10 + LOST_AFTER / 2, LOST_AFTER));
    }

    #[test]
    fn an_unassigned_or_foreign_reply_is_rejected() {
        let now = Instant::now();
        assert!(!DeviceLiveness::new().accept_reply(1, 1, Flags::SYNC_READY, now));
        assert!(!DeviceLiveness::new().accept_reply(1, 3, READY, now));
    }
}
