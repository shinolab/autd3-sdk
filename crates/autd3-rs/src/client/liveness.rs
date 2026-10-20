use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::DeviceState;

use crate::udp::state::SYNCED_FLAGS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceLiveness {
    unanswered_since: Option<Instant>,
    send_failing_since: Option<Instant>,
    state: DeviceState,
    awaits_readmission: bool,
}

impl DeviceLiveness {
    pub(crate) const fn new() -> Self {
        Self {
            unanswered_since: None,
            send_failing_since: None,
            state: DeviceState::Ready,
            awaits_readmission: false,
        }
    }

    pub(crate) fn readmit(&mut self) {
        self.awaits_readmission = false;
    }

    pub(crate) fn accepts_frames(self) -> bool {
        !self.is_lost() && !self.awaits_readmission
    }

    pub(crate) fn record_send_failure(&mut self, now: Instant, lost_timeout: Duration) {
        let since = *self.send_failing_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= lost_timeout {
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn start_unanswered_clock(&mut self, now: Instant) -> bool {
        let starts_afresh = self.unanswered_since.is_none();
        self.unanswered_since.get_or_insert(now);
        starts_afresh
    }

    pub(crate) fn cancel_unanswered_clock(&mut self, requested_at: Instant) {
        if self.unanswered_since == Some(requested_at) {
            self.unanswered_since = None;
        }
    }

    pub(crate) fn accept_reply(
        &mut self,
        expected_unit_id: u8,
        reported_unit_id: u8,
        flags: Flags,
    ) -> bool {
        if self.is_lost() {
            return false;
        }
        if !flags.contains(Flags::ASSIGNED) || reported_unit_id != expected_unit_id {
            self.state = DeviceState::Lost;
            return false;
        }
        self.apply_reply_flags(flags);
        true
    }

    pub(crate) fn try_revive(
        &mut self,
        expected_unit_id: u8,
        reported_unit_id: u8,
        flags: Flags,
    ) -> bool {
        if !self.is_lost()
            || !flags.contains(Flags::ASSIGNED)
            || reported_unit_id != expected_unit_id
        {
            return false;
        }
        self.apply_reply_flags(flags);
        self.awaits_readmission = true;
        true
    }

    fn apply_reply_flags(&mut self, flags: Flags) {
        self.unanswered_since = None;
        self.send_failing_since = None;
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
        assert!(liveness.accept_reply(1, 1, READY));
        assert!(liveness.start_unanswered_clock(now + LOST_AFTER));
    }

    #[test]
    fn a_request_that_never_left_does_not_count_towards_lost() {
        let mut liveness = DeviceLiveness::new();
        let now = Instant::now();
        liveness.start_unanswered_clock(now);
        liveness.cancel_unanswered_clock(now);
        liveness.mark_lost_if_unanswered(now + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn an_earlier_unanswered_request_outlives_a_later_one_that_never_left() {
        let mut liveness = DeviceLiveness::new();
        let earlier = Instant::now();
        let later = earlier + Duration::from_millis(1);
        liveness.start_unanswered_clock(earlier);
        liveness.start_unanswered_clock(later);
        liveness.cancel_unanswered_clock(later);
        liveness.mark_lost_if_unanswered(earlier + LOST_AFTER, LOST_AFTER);
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
        assert!(liveness.accept_reply(1, 1, READY));
        liveness.record_send_failure(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.record_send_failure(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn a_ready_reply_is_ready() {
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(2, 2, READY));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_pulse_is_syncing() {
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(0, 0, Flags::ASSIGNED));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(0, 0, READY));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_ptp_lock_is_syncing() {
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::SYNC_READY));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(1, 1, Flags::ASSIGNED | Flags::PTP_LOCKED));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(1, 1, READY));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unanswered_request_past_the_lost_timeout_makes_a_device_lost_for_good() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        liveness.start_unanswered_clock(start);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        assert!(liveness.accept_reply(1, 1, READY));
        let later = start + LOST_AFTER / 2;
        liveness.start_unanswered_clock(later);
        liveness.start_unanswered_clock(later + LOST_AFTER / 2);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.mark_lost_if_unanswered(later + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
        assert!(!liveness.accept_reply(1, 1, READY));
        assert_eq!(liveness.state(), DeviceState::Lost);
    }

    #[test]
    fn a_recovered_device_counts_its_silence_afresh() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        liveness.start_unanswered_clock(start);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Lost);
        assert!(liveness.try_revive(1, 1, Flags::ASSIGNED));
        assert_eq!(liveness.state(), DeviceState::Syncing);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Syncing);
        assert!(liveness.accept_reply(1, 1, READY));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn a_recovered_device_takes_no_frame_until_it_is_readmitted() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accepts_frames());
        liveness.start_unanswered_clock(start);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER, LOST_AFTER);
        assert!(!liveness.accepts_frames());
        assert!(liveness.try_revive(1, 1, READY));
        assert!(!liveness.is_lost());
        assert!(!liveness.accepts_frames());
        liveness.readmit();
        assert!(liveness.accepts_frames());
    }

    #[test]
    fn only_a_lost_device_answering_under_its_own_id_is_revived() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(!liveness.try_revive(1, 1, READY));
        assert!(liveness.accepts_frames());
        liveness.start_unanswered_clock(start);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER, LOST_AFTER);
        assert!(!liveness.try_revive(1, 1, Flags::SYNC_READY));
        assert!(!liveness.try_revive(1, 2, READY));
        assert_eq!(liveness.state(), DeviceState::Lost);
        assert!(liveness.try_revive(1, 1, READY));
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn the_lost_deadline_follows_the_first_unanswered_request_until_the_device_is_lost() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
        liveness.start_unanswered_clock(start);
        liveness.start_unanswered_clock(start + LOST_AFTER / 2);
        assert_eq!(liveness.lost_deadline(LOST_AFTER), Some(start + LOST_AFTER));
        assert!(liveness.accept_reply(1, 1, READY));
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
        liveness.start_unanswered_clock(start + LOST_AFTER);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(liveness.lost_deadline(LOST_AFTER), None);
    }

    #[test]
    fn silence_without_a_request_is_not_lost() {
        let start = Instant::now();
        let mut liveness = DeviceLiveness::new();
        assert!(liveness.accept_reply(1, 1, READY));
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 10, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
        liveness.start_unanswered_clock(start + LOST_AFTER * 10);
        liveness.mark_lost_if_unanswered(start + LOST_AFTER * 10 + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(liveness.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unassigned_or_foreign_reply_is_lost() {
        let mut liveness = DeviceLiveness::new();
        assert!(!liveness.accept_reply(1, 1, Flags::SYNC_READY));
        assert_eq!(liveness.state(), DeviceState::Lost);

        let mut liveness = DeviceLiveness::new();
        assert!(!liveness.accept_reply(1, 3, READY));
        assert_eq!(liveness.state(), DeviceState::Lost);
    }
}
