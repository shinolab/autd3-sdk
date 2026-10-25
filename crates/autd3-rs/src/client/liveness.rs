use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::DeviceState;

use crate::udp::state::{PTP_FLAGS, SYNCED_FLAGS};

#[derive(Clone, Copy)]
pub(crate) struct DeviceLiveness {
    unanswered_since: Option<Instant>,
    answered_at: Option<Instant>,
    send_failing_since: Option<Instant>,
    state: DeviceState,
    last_flags: Flags,
}

impl DeviceLiveness {
    pub(crate) const fn new() -> Self {
        Self {
            unanswered_since: None,
            answered_at: None,
            send_failing_since: None,
            state: DeviceState::Ready,
            last_flags: SYNCED_FLAGS,
        }
    }

    pub(crate) fn record_send_failure(&mut self, now: Instant, lost_timeout: Duration) {
        let since = *self.send_failing_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= lost_timeout {
            self.state = DeviceState::Lost;
        }
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
        expected_unit_id: u8,
        reported_unit_id: u8,
        flags: Flags,
        now: Instant,
    ) -> bool {
        if self.is_lost() {
            return false;
        }
        if !flags.contains(Flags::ASSIGNED) || reported_unit_id != expected_unit_id {
            self.state = DeviceState::Lost;
            return false;
        }
        self.apply_reply_flags(flags, now);
        true
    }

    fn apply_reply_flags(&mut self, flags: Flags, now: Instant) {
        self.unanswered_since = None;
        self.answered_at = Some(now);
        self.send_failing_since = None;
        self.last_flags = flags;
        self.state = if flags.contains(SYNCED_FLAGS) {
            DeviceState::Ready
        } else {
            DeviceState::Syncing
        };
    }

    pub(crate) fn mark_lost_if_unanswered(&mut self, now: Instant, lost_timeout: Duration) {
        if self
            .unanswered_since
            .is_some_and(|since| now.saturating_duration_since(since) >= lost_timeout)
        {
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn lost_deadline(self, lost_timeout: Duration) -> Option<Instant> {
        if self.is_lost() {
            return None;
        }
        self.unanswered_since
            .and_then(|since| since.checked_add(lost_timeout))
    }

    pub(crate) fn state(self) -> DeviceState {
        self.state
    }

    pub(crate) fn ptp_flags(self) -> Flags {
        if self.is_lost() {
            return Flags::empty();
        }
        self.last_flags.intersection(PTP_FLAGS)
    }

    pub(crate) fn is_lost(self) -> bool {
        self.state == DeviceState::Lost
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
        liveness.mark_lost_if_unanswered(requested_at + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_older_than_the_request_does_not_stop_its_clock() {
        let mut liveness = DeviceLiveness::new();
        let answered_at = Instant::now();
        let requested_at = answered_at + Duration::from_micros(1);
        assert!(liveness.accept_reply(1, 1, READY, answered_at));
        assert!(liveness.start_unanswered_clock(requested_at));
        liveness.mark_lost_if_unanswered(requested_at + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn a_send_that_fails_once_does_not_make_a_device_lost() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        liveness.record_send_failure(now, LOST_AFTER);
        liveness.mark_lost_if_unanswered(now + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn sends_that_keep_failing_past_the_lost_timeout_make_a_device_lost() {
        let mut liveness = DeviceLiveness::new();
        let start = Instant::now();
        liveness.record_send_failure(start, LOST_AFTER);
        liveness.record_send_failure(start + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.record_send_failure(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn a_reply_between_two_failed_sends_restarts_the_count() {
        let mut liveness = DeviceLiveness::new();
        let start = Instant::now();
        liveness.record_send_failure(start, LOST_AFTER);
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 2));
        liveness.record_send_failure(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.record_send_failure(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn the_ptp_flags_follow_the_last_reply() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert_eq!(liveness.ptp_flags(), Flags::PTP_LOCKED);

        assert!(liveness.accept_reply(1, 1, READY | Flags::PTP_FAILSAFE, now));
        assert_eq!(
            liveness.ptp_flags(),
            Flags::PTP_LOCKED | Flags::PTP_FAILSAFE
        );

        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::PTP_FAILSAFE, now));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert_eq!(liveness.ptp_flags(), Flags::PTP_FAILSAFE);

        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::SYNC_READY, now));
        assert_eq!(liveness.ptp_flags(), Flags::empty());
    }

    #[test]
    fn a_lost_device_reports_no_ptp_flags() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        assert!(liveness.accept_reply(1, 1, READY | Flags::PTP_FAILSAFE, now));
        liveness.start_unanswered_clock(now + Duration::from_micros(1));
        liveness.mark_lost_if_unanswered(now + Duration::from_micros(1) + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
        assert_eq!(liveness.ptp_flags(), Flags::empty());
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
    fn an_unanswered_request_past_the_lost_timeout_makes_a_device_lost_for_good() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        liveness.start_unanswered_clock(start);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 4));
        let later = start + LOST_AFTER / 2;
        liveness.start_unanswered_clock(later);
        liveness.start_unanswered_clock(later + LOST_AFTER / 2);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.mark_lost_if_unanswered(later + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
        assert!(!liveness.accept_reply(1, 1, READY, later + LOST_AFTER));
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn the_lost_deadline_follows_the_first_unanswered_request_until_the_device_is_lost() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
        liveness.start_unanswered_clock(start);
        liveness.start_unanswered_clock(start + LOST_AFTER / 2);
        assert_eq!(liveness.lost_deadline(LOST_AFTER), Some(start + LOST_AFTER));
        assert!(liveness.accept_reply(1, 1, READY, start + LOST_AFTER / 2));
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
        liveness.start_unanswered_clock(start + LOST_AFTER);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
    }

    #[test]
    fn silence_without_a_request_is_not_lost() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(1, 1, READY, start));
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 10, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.start_unanswered_clock(start + LOST_AFTER * 10);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 10 + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unassigned_or_foreign_reply_is_lost() {
        let now = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(!liveness.accept_reply(1, 1, Flags::SYNC_READY, now));
        assert_eq!(liveness.state(), DeviceState::Lost);

        let mut liveness = DeviceLiveness::new();
        assert!(!liveness.accept_reply(1, 3, READY, now));
        assert_eq!(liveness.state(), DeviceState::Lost);
    }
}
