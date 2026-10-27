use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use autd3_cpu_wire::payload::Telemetry;

use super::Cpu;
use crate::cmd;
use crate::port::Port;
use crate::utils::OptionalAtomic;

pub(super) struct Health {
    pub(super) fifo_drop: AtomicU32,
    pub(super) dispatch_error: Cell<u32>,
    pub(super) processed: Cell<u32>,
    pub(super) send_failure: AtomicU32,
    pub(super) exec_late: Cell<u32>,
    pub(super) failsafe: Cell<u16>,
    pub(super) failsafe_fired: Cell<bool>,
    pub(super) ptp_unlock_failsafe: Cell<u16>,
    pub(super) ptp_unlock_failsafe_fired: Cell<bool>,
    pub(super) ptp_failsafe_active: AtomicBool,
    pub(super) ptp_unlocked_ms: OptionalAtomic<u32>,
    pub(super) host_idle_ms: OptionalAtomic<u32>,
    pub(super) boot_failure: Cell<u8>,
}

impl Health {
    pub(super) const fn new() -> Self {
        Self {
            fifo_drop: AtomicU32::new(0),
            dispatch_error: Cell::new(0),
            processed: Cell::new(0),
            send_failure: AtomicU32::new(0),
            exec_late: Cell::new(0),
            failsafe: Cell::new(0),
            failsafe_fired: Cell::new(false),
            ptp_unlock_failsafe: Cell::new(0),
            ptp_unlock_failsafe_fired: Cell::new(false),
            ptp_failsafe_active: AtomicBool::new(false),
            ptp_unlocked_ms: OptionalAtomic::new(),
            host_idle_ms: OptionalAtomic::new(),
            boot_failure: Cell::new(0),
        }
    }
}

impl Cpu {
    pub fn count_boot_failure(&self) {
        let count = &self.health.boot_failure;
        count.set(count.get().saturating_add(1));
    }

    pub fn count_send_failures(&self, count: u32) {
        self.health.send_failure.fetch_add(count, Ordering::Relaxed);
    }

    pub fn publish_ptp_unlocked(&self, elapsed_ms: Option<u32>) {
        self.health.ptp_unlocked_ms.store(elapsed_ms);
    }

    pub fn publish_host_idle(&self, idle_ms: Option<u32>) {
        self.health.host_idle_ms.store(idle_ms);
    }

    #[must_use]
    pub fn ptp_failsafe_active(&self) -> bool {
        self.health.ptp_failsafe_active.load(Ordering::Relaxed)
    }

    pub(crate) fn lower_ptp_failsafe(&self) {
        self.health
            .ptp_failsafe_active
            .store(false, Ordering::Relaxed);
    }

    #[must_use]
    pub(crate) fn ptp_unlocked_ms(&self) -> Option<u32> {
        self.health.ptp_unlocked_ms.load()
    }

    #[must_use]
    pub(crate) fn host_idle_ms(&self) -> Option<u32> {
        self.health.host_idle_ms.load()
    }

    pub(crate) fn reset_telemetry(&self) {
        let health = &self.health;
        health.fifo_drop.store(0, Ordering::Relaxed);
        health.dispatch_error.set(0);
        health.processed.set(0);
        health.send_failure.store(0, Ordering::Relaxed);
        health.exec_late.set(0);
        health.failsafe.set(0);
        health.failsafe_fired.set(false);
        health.ptp_unlock_failsafe.set(0);
        health.ptp_unlock_failsafe_fired.set(false);
    }

    pub(crate) fn telemetry(&self, sync_resync: u8, cpu_watchdog: u8) -> Telemetry {
        let health = &self.health;
        Telemetry {
            fifo_drop: health.fifo_drop.load(Ordering::Relaxed).into(),
            dispatch_error: health.dispatch_error.get().into(),
            processed: health.processed.get().into(),
            send_failure: health.send_failure.load(Ordering::Relaxed).into(),
            failsafe: health.failsafe.get().into(),
            ptp_unlock_failsafe: health.ptp_unlock_failsafe.get().into(),
            sync_resync,
            boot_failure: health.boot_failure.get(),
            cpu_watchdog,
            exec_late: health.exec_late.get().into(),
        }
    }

    pub(super) fn check_failsafes<P: Port>(&self, port: &mut P) {
        fn failsafe<P: Port>(
            port: &mut P,
            tripped: bool,
            fired: &Cell<bool>,
            counter: &Cell<u16>,
        ) -> bool {
            if !tripped {
                fired.set(false);
                return false;
            }
            if fired.get() {
                return false;
            }
            fired.set(true);
            cmd::failsafe::mute(port);
            counter.set(counter.get().saturating_add(1));
            true
        }

        let config = self.config();
        let health = &self.health;
        let silent = cmd::failsafe::host_silent(self, &config);
        failsafe(port, silent, &health.failsafe_fired, &health.failsafe);
        let unlocked = cmd::failsafe::ptp_unlock_expired(self, &config);
        if failsafe(
            port,
            unlocked,
            &health.ptp_unlock_failsafe_fired,
            &health.ptp_unlock_failsafe,
        ) {
            health.ptp_failsafe_active.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::app::fifo::FIFO_DEPTH;
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::force_fan;
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn a_clean_boot_counts_no_boot_failure() {
        let mut h = Harness::new();
        h.reboot();
        assert_eq!(h.telemetry().boot_failure, 0);
    }

    #[test]
    fn an_fpga_timeout_at_boot_is_counted_and_survives_clear() {
        let mut h = Harness::new();
        h.port.latch_stuck = true;
        h.reboot();
        h.port.latch_stuck = false;
        assert_eq!(h.telemetry().boot_failure, 1);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry().boot_failure, 1);
    }

    #[test]
    fn a_flash_failure_while_marking_the_boot_is_counted() {
        let mut h = Harness::new();
        h.port.flash_fail = true;
        h.reboot();
        h.port.flash_fail = false;
        assert_eq!(h.telemetry().boot_failure, 1);
    }

    #[test]
    fn boot_failures_reported_by_the_board_accumulate() {
        let h = Harness::new();
        h.cpu.count_boot_failure();
        h.cpu.count_boot_failure();
        assert_eq!(h.telemetry().boot_failure, 2);
    }

    #[test]
    fn send_failures_are_counted_and_cleared_by_clear() {
        let mut h = Harness::new();
        h.cpu.count_send_failures(2);
        h.cpu.count_send_failures(1);
        assert_eq!(h.telemetry().send_failure, 3);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.telemetry().send_failure, 0);
    }

    #[test]
    fn telemetry_counts_processed_frames() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(Cmd::Nop));
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.telemetry().processed, 2);
    }

    #[test]
    fn telemetry_counts_dispatch_errors() {
        let mut h = Harness::new();
        h.deliver(&force_fan(2));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.telemetry().dispatch_error, 1);
    }

    #[test]
    fn telemetry_counts_fifo_drops() {
        let mut h = Harness::new();

        for _ in 0..FIFO_DEPTH {
            h.deliver_no_drain(&Frame::new(Cmd::Nop));
        }
        assert_eq!(h.telemetry().fifo_drop, 1);
    }
}
