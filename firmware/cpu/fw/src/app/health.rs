use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use autd3_cpu_wire::payload::Telemetry;

use super::Cpu;
use crate::cmd;
use crate::fpga;
use crate::fpga_params::{ADDR_FPGA_STATE, FpgaStateFlags};
use crate::port::Port;
use crate::utils::OptionalAtomic;

pub(super) struct Health {
    pub(super) fifo_drop: AtomicU32,
    pub(super) exec_late: AtomicU32,
    pub(super) sync_resync: AtomicU8,
    pub(super) cpu_watchdog: AtomicU8,
    pub(super) boot_failure: AtomicU8,
    pub(super) fpga_state: AtomicU8,
    pub(super) failsafe_fired: Cell<bool>,
    pub(super) ptp_unlock_failsafe_fired: Cell<bool>,
    pub(super) failsafe_active: AtomicBool,
    pub(super) ptp_unlocked_ms: OptionalAtomic<u32>,
    pub(super) host_idle_ms: OptionalAtomic<u32>,
}

impl Health {
    pub(super) const fn new() -> Self {
        Self {
            fifo_drop: AtomicU32::new(0),
            exec_late: AtomicU32::new(0),
            sync_resync: AtomicU8::new(0),
            cpu_watchdog: AtomicU8::new(0),
            boot_failure: AtomicU8::new(0),
            fpga_state: AtomicU8::new(0),
            failsafe_fired: Cell::new(false),
            ptp_unlock_failsafe_fired: Cell::new(false),
            failsafe_active: AtomicBool::new(false),
            ptp_unlocked_ms: OptionalAtomic::new(),
            host_idle_ms: OptionalAtomic::new(),
        }
    }
}

impl Cpu {
    pub fn count_boot_failure(&self) {
        let count = &self.health.boot_failure;
        count.store(
            count.load(Ordering::Relaxed).saturating_add(1),
            Ordering::Relaxed,
        );
    }

    pub(super) fn count_exec_late(&self) {
        let count = &self.health.exec_late;
        count.store(
            count.load(Ordering::Relaxed).wrapping_add(1),
            Ordering::Relaxed,
        );
    }

    pub fn publish_ptp_unlocked(&self, elapsed_ms: Option<u32>) {
        self.health.ptp_unlocked_ms.store(elapsed_ms);
    }

    pub fn publish_host_idle(&self, idle_ms: Option<u32>) {
        self.health.host_idle_ms.store(idle_ms);
    }

    #[must_use]
    pub fn failsafe_active(&self) -> bool {
        self.health.failsafe_active.load(Ordering::Relaxed)
    }

    pub(crate) fn mute_output<P: Port>(&self, port: &mut P) {
        cmd::failsafe::mute(port);
        self.health.failsafe_active.store(true, Ordering::Relaxed);
    }

    pub(crate) fn lower_failsafe(&self) {
        self.health.failsafe_active.store(false, Ordering::Relaxed);
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
        health.exec_late.store(0, Ordering::Relaxed);
        health.failsafe_fired.set(false);
        health.ptp_unlock_failsafe_fired.set(false);
    }

    #[must_use]
    pub fn telemetry(&self) -> Telemetry {
        let health = &self.health;
        Telemetry {
            fifo_drop: health.fifo_drop.load(Ordering::Relaxed).into(),
            exec_late: health.exec_late.load(Ordering::Relaxed).into(),
            sync_resync: health.sync_resync.load(Ordering::Relaxed),
            cpu_watchdog: health.cpu_watchdog.load(Ordering::Relaxed),
            boot_failure: health.boot_failure.load(Ordering::Relaxed),
        }
    }

    #[must_use]
    pub fn fpga_state(&self) -> u8 {
        self.health.fpga_state.load(Ordering::Relaxed)
    }

    pub(super) fn sample_fpga_state<P: Port>(&self, port: &mut P) {
        let health = &self.health;
        let [state, resync] = fpga::read_ctl(port, ADDR_FPGA_STATE).to_le_bytes();
        let state = FpgaStateFlags::from_bits_retain(state) - FpgaStateFlags::FAILSAFE;
        health.fpga_state.store(state.bits(), Ordering::Relaxed);
        health.sync_resync.store(resync, Ordering::Relaxed);
    }

    pub(super) fn sample_fpga_counters<P: Port>(&self, port: &mut P) {
        self.sample_fpga_state(port);
        self.health
            .cpu_watchdog
            .store(fpga::watchdog_state(port).count, Ordering::Relaxed);
    }

    pub(super) fn check_failsafes<P: Port>(&self, port: &mut P) {
        fn failsafe<P: Port>(cpu: &Cpu, port: &mut P, tripped: bool, fired: &Cell<bool>) {
            if !tripped {
                fired.set(false);
                return;
            }
            if fired.get() {
                return;
            }
            fired.set(true);
            cpu.mute_output(port);
        }

        let config = self.config();
        let health = &self.health;
        let silent = cmd::failsafe::host_silent(self, &config);
        failsafe(self, port, silent, &health.failsafe_fired);
        let unlocked = cmd::failsafe::ptp_unlock_expired(self, &config);
        failsafe(self, port, unlocked, &health.ptp_unlock_failsafe_fired);
        if !self.failsafe_active() && fpga::watchdog_state(port).fired {
            health.failsafe_active.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::app::fifo::FIFO_DEPTH;
    use crate::fpga_params::{ADDR_FPGA_STATE, ADDR_FUNCTION_BITS, FunctionBits};
    use crate::proto::{Cmd, Error};
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
    fn telemetry_counts_fifo_drops_until_a_clear() {
        let mut h = Harness::new();

        for _ in 0..FIFO_DEPTH {
            h.deliver_no_drain(&Frame::new(Cmd::Nop));
        }
        assert_eq!(h.telemetry().fifo_drop, 1);

        while h.process_one() {}
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.telemetry().fifo_drop, 0);
    }

    #[test]
    fn boot_failures_saturate_instead_of_wrapping() {
        let h = Harness::new();
        for _ in 0..=u32::from(u8::MAX) {
            h.cpu.count_boot_failure();
        }
        assert_eq!(h.telemetry().boot_failure, u8::MAX);
    }

    #[test]
    fn a_tick_samples_the_resync_count_from_the_fpga_state_high_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
        assert_eq!(h.telemetry().sync_resync, 0);

        h.tick_1ms(1);
        assert_eq!(h.telemetry().sync_resync, 0x2A);
    }

    #[test]
    fn a_tick_samples_the_state_flags_from_the_fpga_state_low_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A7F);
        assert_eq!(h.cpu.fpga_state(), 0);

        h.tick_1ms(1);
        assert_eq!(h.cpu.fpga_state(), 0x7F);
    }

    #[test]
    fn a_completed_frame_resamples_the_fpga_state_before_its_reply() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x0506);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.cpu.fpga_state(), 0x06);
        assert_eq!(h.telemetry().sync_resync, 0x05);
    }

    #[test]
    fn the_failsafe_bit_of_the_fpga_state_is_never_sampled() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x83);
        h.tick_1ms(1);
        assert_eq!(h.cpu.fpga_state(), 0x03);
    }

    #[test]
    fn a_tick_samples_the_watchdog_fire_count_which_survives_clear() {
        let mut h = Harness::new();
        h.port.watchdog_state = 0x2A01;
        h.tick_1ms(1);
        assert_eq!(h.telemetry().cpu_watchdog, 0x2A);

        h.deliver(&Frame::new(Cmd::Clear));
        h.tick_1ms(1);
        assert_eq!(h.telemetry().cpu_watchdog, 0x2A);
    }

    #[test]
    fn an_fpga_without_the_watchdog_reports_no_fires() {
        let mut h = Harness::new();
        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::STRICT_SILENCER_GUARD.bits()),
        );
        h.port.watchdog_state = 0x2A01;
        h.tick_1ms(1);
        assert_eq!(h.telemetry().cpu_watchdog, 0);
    }
}
