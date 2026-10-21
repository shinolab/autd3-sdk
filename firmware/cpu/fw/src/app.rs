use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, Ordering};

use crate::cmd;
use crate::ctx::MainCell;
use crate::fifo::{FIFO_DEPTH, Fifo};
use crate::fpga;
use crate::port::Port;
use crate::proto::{
    Cmd, Disposition, Drained, EXEC_AT_UNIT_NS, Error, FrameHeader, PAYLOAD_BYTES, Reply,
    ReplyData, RxFrame,
};
use autd3_cpu_wire::payload::Telemetry;
use zerocopy::little_endian::U32;

#[repr(C, align(4))]
struct Slot {
    msg_id: Cell<u16>,
    len: Cell<u16>,
    payload: Cell<[u8; PAYLOAD_BYTES]>,
    header: Cell<FrameHeader>,
}

const _: () = assert!(core::mem::offset_of!(Slot, payload).is_multiple_of(4));

impl Slot {
    const fn new() -> Self {
        Self {
            msg_id: Cell::new(0),
            len: Cell::new(0),
            payload: Cell::new([0; PAYLOAD_BYTES]),
            header: Cell::new(FrameHeader {
                reserved: 0,
                cmd: 0,
                exec_at: U32::ZERO,
            }),
        }
    }

    fn store(&self, header: FrameHeader, payload: &[u8], msg_id: u16) {
        let cells = self.payload.as_array_of_cells();
        let len = payload.len().min(cells.len());
        for (cell, byte) in cells.iter().zip(&payload[..len]) {
            cell.set(*byte);
        }
        self.len.set(len as u16);
        self.msg_id.set(msg_id);
        self.header.set(header);
    }

    fn load(&self, frame: &mut RxFrame) {
        let len = usize::from(self.len.get()).min(PAYLOAD_BYTES);
        for (byte, cell) in frame.payload[..len]
            .iter_mut()
            .zip(self.payload.as_array_of_cells())
        {
            *byte = cell.get();
        }
        frame.len = len as u16;
        frame.msg_id = self.msg_id.get();
        frame.header = self.header.get();
    }
}

struct Counters {
    fifo_drop: AtomicU32,
    dispatch_error: AtomicU32,
    processed: AtomicU32,
    send_failure: AtomicU32,
    exec_late: AtomicU32,
    failsafe: AtomicU16,
    ptp_unlock_failsafe: AtomicU16,
    boot_failure: AtomicU8,
}

impl Counters {
    const fn new() -> Self {
        Self {
            fifo_drop: AtomicU32::new(0),
            dispatch_error: AtomicU32::new(0),
            processed: AtomicU32::new(0),
            send_failure: AtomicU32::new(0),
            exec_late: AtomicU32::new(0),
            failsafe: AtomicU16::new(0),
            ptp_unlock_failsafe: AtomicU16::new(0),
            boot_failure: AtomicU8::new(0),
        }
    }

    fn reset_all_but_boot_failure(&self) {
        self.fifo_drop.store(0, Ordering::Relaxed);
        self.dispatch_error.store(0, Ordering::Relaxed);
        self.processed.store(0, Ordering::Relaxed);
        self.send_failure.store(0, Ordering::Relaxed);
        self.exec_late.store(0, Ordering::Relaxed);
        self.failsafe.store(0, Ordering::Relaxed);
        self.ptp_unlock_failsafe.store(0, Ordering::Relaxed);
    }

    fn telemetry(&self, sync_resync: u8, cpu_watchdog: u8) -> Telemetry {
        Telemetry {
            fifo_drop: self.fifo_drop.load(Ordering::Relaxed).into(),
            dispatch_error: self.dispatch_error.load(Ordering::Relaxed).into(),
            processed: self.processed.load(Ordering::Relaxed).into(),
            send_failure: self.send_failure.load(Ordering::Relaxed).into(),
            failsafe: self.failsafe.load(Ordering::Relaxed).into(),
            ptp_unlock_failsafe: self.ptp_unlock_failsafe.load(Ordering::Relaxed).into(),
            sync_resync,
            boot_failure: self.boot_failure.load(Ordering::Relaxed),
            cpu_watchdog,
            exec_late: self.exec_late.load(Ordering::Relaxed).into(),
        }
    }
}

fn bump(counter: &AtomicU32) {
    counter.fetch_add(1, Ordering::Relaxed);
}

fn bump_saturating_u16(counter: &AtomicU16) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
        count.checked_add(1)
    });
}

fn bump_saturating_u8(counter: &AtomicU8) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
        count.checked_add(1)
    });
}

pub struct Cpu {
    slots: [Slot; FIFO_DEPTH as usize],
    fifo: Fifo,
    counters: Counters,
    failsafe_fired: MainCell<bool>,
    ptp_unlock_failsafe_fired: MainCell<bool>,
    ptp_failsafe_active: AtomicBool,
    pub(crate) config: MainCell<cmd::cpu_config::CpuConfig>,
    pub(crate) config_reset_requested: AtomicBool,
    ptp_unlocked: AtomicBool,
    ptp_unlocked_ms: AtomicU32,
    pub(crate) update: cmd::update::UpdateSession,
    pub(crate) fpga_update: cmd::fpga_update::FpgaUpdateSession,
    results: [Cell<Reply>; FIFO_DEPTH as usize],
}

const _: () = assert!(FIFO_DEPTH as usize > autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES);

type Outcome = Result<ReplyData, Error>;

pub(crate) const EXEC_SPIN_NS: i64 = 50_000;

fn ahead_of_exec_at_ns(now: u64, exec_at: u32) -> i64 {
    let ahead_us = exec_at
        .wrapping_sub((now / EXEC_AT_UNIT_NS) as u32)
        .cast_signed();
    i64::from(ahead_us) * EXEC_AT_UNIT_NS.cast_signed() - (now % EXEC_AT_UNIT_NS).cast_signed()
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    #[must_use]
    #[const_fn::const_fn(cfg(not(loom)))]
    pub const fn new() -> Self {
        Self {
            slots: [const { Slot::new() }; FIFO_DEPTH as usize],
            fifo: Fifo::new(),
            counters: Counters::new(),
            failsafe_fired: MainCell::new(false),
            ptp_unlock_failsafe_fired: MainCell::new(false),
            ptp_failsafe_active: AtomicBool::new(false),
            config: MainCell::new(cmd::cpu_config::default_config()),
            config_reset_requested: AtomicBool::new(false),
            ptp_unlocked: AtomicBool::new(false),
            ptp_unlocked_ms: AtomicU32::new(0),
            update: cmd::update::UpdateSession::new(),
            fpga_update: cmd::fpga_update::FpgaUpdateSession::new(),
            results: [const { Cell::new(Reply::EMPTY) }; FIFO_DEPTH as usize],
        }
    }
}

impl Cpu {
    pub fn init<P: Port>(&self, port: &mut P) {
        let config = self.config();
        if fpga::init(port, config.fpga_wait_update_max_polls).is_ok() {
            self.lower_ptp_failsafe();
        } else {
            self.count_boot_failure();
        }
        fpga::set_watchdog_timeout(port, config.cpu_watchdog_timeout);
    }

    pub fn mark_boot_attempt<P: Port>(&self, port: &mut P) {
        if self.record_boot_attempt(port).is_err() {
            self.count_boot_failure();
        }
    }

    pub fn count_boot_failure(&self) {
        bump_saturating_u8(&self.counters.boot_failure);
    }

    pub fn count_send_failures(&self, count: u32) {
        self.counters
            .send_failure
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn publish_ptp_unlocked(&self, elapsed_ms: Option<u32>) {
        match elapsed_ms {
            Some(ms) => {
                self.ptp_unlocked_ms.store(ms, Ordering::Relaxed);
                self.ptp_unlocked.store(true, Ordering::Release);
            }
            None => self.ptp_unlocked.store(false, Ordering::Release),
        }
    }

    #[must_use]
    pub fn ptp_failsafe_active(&self) -> bool {
        self.ptp_failsafe_active.load(Ordering::Relaxed)
    }

    pub(crate) fn lower_ptp_failsafe(&self) {
        self.ptp_failsafe_active.store(false, Ordering::Relaxed);
    }

    #[must_use]
    pub fn ptp_unlocked_ms(&self) -> Option<u32> {
        self.ptp_unlocked
            .load(Ordering::Acquire)
            .then(|| self.ptp_unlocked_ms.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn booted_slot(&self) -> Option<autd3_cpu_wire::update::Slot> {
        self.update.boot_slot()
    }

    pub(crate) fn reset_telemetry(&self) {
        self.counters.reset_all_but_boot_failure();
        self.failsafe_fired.set(false);
        self.ptp_unlock_failsafe_fired.set(false);
    }

    pub(crate) fn telemetry(&self, sync_resync: u8, cpu_watchdog: u8) -> Telemetry {
        self.counters.telemetry(sync_resync, cpu_watchdog)
    }

    pub fn tick_1ms<P: Port>(&self, port: &mut P) {
        fpga::watchdog_kick(port);
        self.apply_requested_config_reset(port);
        self.update_tick(port);
        self.fpga_update_tick(port);
        let config = self.config();
        let silent = cmd::failsafe::host_silent(port, &config);
        Self::failsafe(port, silent, &self.failsafe_fired, &self.counters.failsafe);
        let unlocked = cmd::failsafe::ptp_unlock_expired(port, &config);
        if Self::failsafe(
            port,
            unlocked,
            &self.ptp_unlock_failsafe_fired,
            &self.counters.ptp_unlock_failsafe,
        ) {
            self.ptp_failsafe_active.store(true, Ordering::Relaxed);
        }
    }

    fn failsafe<P: Port>(
        port: &mut P,
        tripped: bool,
        fired: &MainCell<bool>,
        counter: &AtomicU16,
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
        bump_saturating_u16(counter);
        true
    }

    #[must_use]
    pub fn reply(&self) -> Reply {
        self.result_before(self.fifo.tail_acquire())
    }

    fn result_before(&self, tail: u16) -> Reply {
        self.results[Fifo::slot(tail.wrapping_sub(1))].get()
    }

    pub fn recv_frame(&self, frame: &[u8], msg_id: u16) -> Disposition {
        let Some((header, payload)) = FrameHeader::parse(frame) else {
            return Disposition::Dropped;
        };
        let head = self.fifo.head();
        if Fifo::is_full(head, self.fifo.tail_acquire()) {
            bump(&self.counters.fifo_drop);
            return Disposition::Dropped;
        }
        self.slots[Fifo::slot(head)].store(*header, payload, msg_id);
        self.fifo.publish(head);
        Disposition::Deferred
    }

    pub fn process_one<P: Port>(&self, port: &mut P, work: &mut RxFrame) -> Drained {
        let Some(tail) = self.fifo.next() else {
            return Drained::Empty;
        };
        let slot = &self.slots[Fifo::slot(tail)];
        if !self.is_due(port, slot.header.get().exec_at.get()) {
            return Drained::Empty;
        }
        slot.load(work);
        self.apply_requested_config_reset(port);
        let result = self.handle_frame(port, work);
        self.results[Fifo::slot(tail)].set(result);
        self.fifo.commit(tail);
        Drained::Completed {
            msg_id: work.msg_id,
        }
    }

    fn is_due<P: Port>(&self, port: &mut P, exec_at: u32) -> bool {
        if exec_at == 0 {
            return true;
        }
        let Some(now) = port.sys_time() else {
            bump(&self.counters.exec_late);
            return true;
        };
        if ahead_of_exec_at_ns(now, exec_at) > EXEC_SPIN_NS {
            return false;
        }
        let late = port.without_irq(|port| {
            loop {
                let Some(now) = port.sys_time() else {
                    break true;
                };
                let ahead = ahead_of_exec_at_ns(now, exec_at);
                if ahead <= 0 {
                    break ahead <= -EXEC_AT_UNIT_NS.cast_signed();
                }
            }
        });
        if late {
            bump(&self.counters.exec_late);
        }
        true
    }

    pub fn step<P: Port>(&self, port: &mut P, work: &mut RxFrame, elapsed_ms: u32) -> Drained {
        for _ in 0..elapsed_ms {
            self.tick_1ms(port);
        }
        self.process_one(port, work)
    }

    pub fn process_pending<P: Port>(&self, port: &mut P) {
        let mut work = RxFrame::ZERO;
        while self.process_one(port, &mut work) != Drained::Empty {}
    }

    fn handle_frame<P: Port>(&self, port: &mut P, in_frame: &RxFrame) -> Reply {
        let outcome = match Cmd::from_u8(in_frame.header.cmd) {
            Some(cmd) => self.dispatch(port, cmd, in_frame.payload()),
            None => Err(Error::UnknownCmd),
        };
        let result = match outcome {
            Ok(data) => Reply::new(Error::None, data),
            Err(err) => {
                bump(&self.counters.dispatch_error);
                Reply::new(err, ReplyData::EMPTY)
            }
        };
        bump(&self.counters.processed);
        result
    }

    fn dispatch<P: Port>(&self, port: &mut P, cmd: Cmd, payload: &[u8]) -> Outcome {
        if self.fpga_update.is_locked() && !cmd::fpga_update::allowed_while_locked(cmd) {
            return Err(Error::FpgaUpdateInProgress);
        }
        let config = self.config();
        let max_polls = config.fpga_wait_update_max_polls;
        let result = match cmd {
            Cmd::Nop => Ok(()),
            Cmd::SetCpuConfig => self.set_cpu_config(port, payload),
            Cmd::ReadFpgaState => return Ok(cmd::fpga_state::read(port)),
            Cmd::ReadTelemetry => return Ok(self.read_telemetry(port)),
            Cmd::ReadFirmwareInfo => return Ok(self.read_firmware_info(port)),
            Cmd::ReadFpgaMemory => return cmd::fpga_memory::read(port, payload),
            Cmd::WriteFpgaMemory => cmd::fpga_memory::write(port, payload),
            Cmd::ReadRunningImage => {
                return self
                    .running_image(port)
                    .map(|image| ReplyData::from_slice(&[image.as_u8()]));
            }
            Cmd::WriteFociBuffer => cmd::write_foci::handle(port, payload),
            Cmd::WritePatternRaw => cmd::write_pattern_raw::handle(port, payload),
            Cmd::WritePatternFused => cmd::write_pattern_fused::handle(port, &config, payload),
            Cmd::WritePatternPhase => cmd::write_pattern_phase::handle(port, payload),
            Cmd::WriteModulationBuffer => cmd::write_mod::handle(port, payload),
            Cmd::ConfigModulation => cmd::config_mod::handle(port, payload),
            Cmd::ConfigPattern => cmd::config_pattern::handle(port, payload),
            Cmd::ActivateModulationBank => cmd::activate_mod_bank::handle(port, &config, payload),
            Cmd::ActivatePatternBank => cmd::activate_pattern_bank::handle(port, &config, payload),
            Cmd::SetSilencer => cmd::silencer::handle(port, payload, max_polls),
            Cmd::SetPhaseCorrection => cmd::phase_corr::handle(port, payload),
            Cmd::SetOutputMask => cmd::output_mask::handle(port, payload),
            Cmd::SetPulseWidthTable => cmd::pwe::handle(port, payload),
            Cmd::EmulateGpioIn => cmd::gpio_in::handle(port, payload),
            Cmd::SetGpioOut => cmd::gpio_out::handle(port, payload, max_polls),
            Cmd::ForceFan => cmd::force_fan::handle(port, payload),
            Cmd::SetFpgaConfig => cmd::fpga_config::handle(port, payload),
            Cmd::ReleaseFailsafe => {
                cmd::failsafe::release(port, &config).inspect(|()| self.lower_ptp_failsafe())
            }
            Cmd::UpdateBegin => self.update_begin(port, payload),
            Cmd::UpdateChunk => self.update_chunk(port, payload),
            Cmd::UpdateCommit => self.update_commit(port),
            Cmd::UpdateActivate => self.update_activate(),
            Cmd::Reboot => {
                self.reboot();
                Ok(())
            }
            Cmd::UpdateConfirm => self.update_confirm(port),
            Cmd::FpgaUpdateBegin => self.fpga_update_begin(port, payload),
            Cmd::FpgaUpdateChunk => self.fpga_update_chunk(port, payload),
            Cmd::FpgaUpdateCommit => self.fpga_update_commit(port),
            Cmd::FpgaUpdateActivate => self.fpga_update_activate(),
            Cmd::Synchronize => cmd::sync::handle(port, &config),
            Cmd::Clear => self.clear(port),
            _ => unreachable!(),
        };
        result.map(|()| ReplyData::EMPTY)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {

    use super::EXEC_SPIN_NS;
    use crate::cmd::config_mod::ConfigModPayload;
    use crate::cmd::force_fan::ForceFanPayload;
    use crate::fifo::FIFO_DEPTH;
    use crate::fpga::{PHASE_CORR_WORDS, PWE_TABLE_SIZE, REP_INFINITE};
    use crate::fpga_params::{
        ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_PATTERN_CYCLE0,
        ADDR_PATTERN_MODE0, ADDR_PATTERN_REP0, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG, ADDR_SILENCER_SET_RESULT,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE, CtlFlags,
        EmissionType, NUM_BANKS, NUM_TRANSDUCERS,
    };
    use crate::proto::{
        Cmd, Disposition, Drained, EXEC_AT_UNIT_NS, Error, FRAME_BYTES_MAX, OUTPUT_MASK_WORDS,
        RxFrame,
    };
    use crate::test_utils::builders::{config_mod, force_fan, write_foci_buffer};
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
    fn unknown_cmd_reports_unknown_cmd_and_is_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0x7F));
        assert_eq!(h.status(), Error::UnknownCmd);
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().dispatch_error, 1);
    }

    #[test]
    fn every_cmd_has_a_dispatch_arm() {
        for &cmd in Cmd::ALL {
            let mut h = Harness::new();
            h.deliver(&Frame::new(cmd));
            assert_ne!(h.status(), Error::UnknownCmd, "{cmd:?}");
        }
    }

    #[test]
    fn nop_completes_without_changing_state() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry().dispatch_error, 0);
    }

    #[test]
    fn unknown_non_streaming_cmd_reports_unknown_cmd() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0xEE));
        assert_eq!(h.status(), Error::UnknownCmd);
    }

    #[test]
    fn consecutive_frames_each_process_immediately() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 1);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 2);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 3);
    }

    #[test]
    fn fixed_length_payloads_must_match_exactly() {
        let mut h = Harness::new();
        let mut long = force_fan(1);
        long.set_payload_byte(size_of::<ForceFanPayload>(), 0);
        h.deliver(&long);
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut short = config_mod(0, 10, 4);
        short.set_len(size_of::<ConfigModPayload>() - 1);
        h.deliver(&short);
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&force_fan(1));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn an_oversized_frame_is_dropped() {
        let h = Harness::new();
        let mut frame = std::vec![0u8; FRAME_BYTES_MAX + 1];
        frame[1] = Cmd::Nop as u8;
        assert_eq!(h.cpu.recv_frame(&frame, 0), Disposition::Dropped);
        assert_eq!(h.processed(), 0);
    }

    #[test]
    fn fifo_mode_defers_processing_until_drained() {
        let mut h = Harness::new();

        h.deliver_no_drain(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 0);

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.processed(), 1);
    }

    #[test]
    fn fifo_mode_drains_in_order() {
        let h = Harness::new();
        let mut port = crate::test_utils::mock::MockPort::new();
        let mut work = RxFrame::ZERO;
        for msg_id in 1..=3 {
            let _ = h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), msg_id);
        }
        for msg_id in 1..=3 {
            assert_eq!(
                h.cpu.process_one(&mut port, &mut work),
                Drained::Completed { msg_id }
            );
        }
        assert_eq!(h.cpu.process_one(&mut port, &mut work), Drained::Empty);
    }

    #[test]
    fn a_duplicated_frame_is_processed_twice() {
        let mut h = Harness::new();
        let frame = Frame::new(Cmd::Nop);
        h.deliver_no_drain(&frame);
        h.deliver_no_drain(&frame);
        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.processed(), 2);
    }

    #[test]
    fn the_reserved_header_byte_is_ignored() {
        let mut h = Harness::new();
        let mut bytes = Frame::new(Cmd::ReadFpgaState).bytes();
        bytes[0] = 0xFF;
        let _ = h.cpu.recv_frame(&bytes, 0);
        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.processed(), 1);
        assert_eq!(h.reply_data().len(), 1);
    }

    #[test]
    fn the_reply_follows_every_completed_frame_across_the_ring_wrap() {
        let mut h = Harness::new();
        for i in 0..(3 * u32::from(FIFO_DEPTH)) {
            h.deliver(&Frame::new(Cmd::ReadFpgaState));
            assert_eq!(h.processed(), 2 * i + 1);
            assert_eq!(h.reply_data().len(), 1);
            h.deliver(&Frame::new(Cmd::Nop));
            assert_eq!(h.reply_data(), []);
        }
    }

    #[test]
    fn the_reply_is_empty_until_the_first_frame_completes() {
        let mut h = Harness::new();
        assert_eq!(h.cpu.reply(), crate::proto::Reply::EMPTY);
        for _ in 0..=FIFO_DEPTH {
            h.deliver(&Frame::new(Cmd::ReadFpgaState));
            assert_eq!(h.reply_data().len(), 1);
        }
        h.reboot();
        assert_eq!(h.cpu.reply(), crate::proto::Reply::EMPTY);
    }

    #[test]
    fn the_isr_sees_the_previous_result_while_a_frame_is_being_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        let previous = h.cpu.reply();

        h.deliver_no_drain(&write_foci_buffer(0, 0, &[0x5A5A]));
        h.arm_isr_frame(Cmd::Nop);
        assert!(h.process_one());

        assert_eq!(h.port.isr_seen_reply, Some(previous));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.reply_data(), []);
    }

    #[test]
    fn fifo_overflow_drops_beyond_capacity_and_accepts_after_drain() {
        let mut h = Harness::new();

        let capacity = u32::from(FIFO_DEPTH - 1);
        for _ in 0..capacity {
            h.deliver_no_drain(&Frame::new(Cmd::Nop));
        }
        h.deliver_no_drain(&Frame::new(Cmd::Nop));

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.processed(), capacity);
        assert_eq!(h.telemetry().fifo_drop, 1);

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), capacity + 1);
    }

    #[test]
    fn a_fifo_frame_completes_with_its_msg_id() {
        let h = Harness::new();
        let frame = Frame::new(Cmd::Nop).bytes();
        assert_eq!(
            h.cpu.recv_frame(&frame, 0x1234),
            crate::proto::Disposition::Deferred
        );
        let mut port = crate::test_utils::mock::MockPort::new();
        let mut work = RxFrame::ZERO;
        assert_eq!(
            h.cpu.process_one(&mut port, &mut work),
            Drained::Completed { msg_id: 0x1234 }
        );
        assert_eq!(h.cpu.process_one(&mut port, &mut work), Drained::Empty);
    }

    #[test]
    fn a_step_completes_one_frame_per_call_with_its_msg_id() {
        let h = Harness::new();
        let mut port = crate::test_utils::mock::MockPort::new();
        let mut work = RxFrame::ZERO;
        let _ = h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0x11);
        let _ = h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0x22);
        assert_eq!(
            h.cpu.step(&mut port, &mut work, 0),
            Drained::Completed { msg_id: 0x11 }
        );
        assert_eq!(
            h.cpu.step(&mut port, &mut work, 3),
            Drained::Completed { msg_id: 0x22 }
        );
        assert_eq!(h.cpu.step(&mut port, &mut work, 0), Drained::Empty);
    }

    #[test]
    fn a_full_fifo_drops_the_frame_without_a_reply() {
        let h = Harness::new();
        for _ in 0..FIFO_DEPTH - 1 {
            assert_eq!(
                h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0),
                crate::proto::Disposition::Deferred
            );
        }
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0),
            crate::proto::Disposition::Dropped
        );
    }

    fn scheduled(exec_at: u32) -> Frame {
        let mut frame = Frame::new(Cmd::Nop);
        frame.set_exec_at(exec_at);
        frame
    }

    const US: u64 = EXEC_AT_UNIT_NS;
    const LATER: u32 = 1_000;
    const SPIN: u64 = EXEC_SPIN_NS.unsigned_abs();

    #[test]
    fn a_frame_beyond_the_spin_window_returns_to_the_main_loop() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time = Some(exec_at - SPIN - 1);
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 0);
        assert!(!h.process_one());
        assert_eq!(h.port.sys_time_reads_masked, 0);

        h.port.sys_time = Some(exec_at);
        assert!(h.process_one());
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_frame_within_the_spin_window_waits_with_irq_masked_until_its_exec_at() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time_reads = [Some(exec_at - SPIN), Some(exec_at - 1), Some(exec_at)].into();
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.port.sys_time_reads_masked, 2);
        assert!(!h.port.irq_masked);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_lost_device_time_during_the_spin_processes_the_frame_as_late() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time_reads = [Some(exec_at - 1), None].into();
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn a_zero_exec_at_is_processed_without_reading_the_time() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(0));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_waiting_frame_holds_back_the_frames_behind_it() {
        let mut h = Harness::new();
        h.port.sys_time = Some(0);
        h.deliver(&scheduled(LATER));
        assert_eq!(
            h.cpu.recv_frame(&scheduled(0).bytes(), 0),
            Disposition::Deferred
        );
        assert!(!h.process_one());
        assert_eq!(h.processed(), 0);

        h.port.sys_time = Some(u64::from(LATER) * US);
        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.processed(), 2);
    }

    #[test]
    fn the_main_loop_keeps_ticking_while_a_frame_waits() {
        let mut h = Harness::new();
        h.port.sys_time = Some(0);
        h.deliver(&scheduled(LATER));
        let kicks = h.port.watchdog_kicks;
        let mut work = RxFrame::ZERO;
        assert_eq!(h.cpu.step(&mut h.port, &mut work, 3), Drained::Empty);
        assert_eq!(h.port.watchdog_kicks, kicks + 3);
        assert_eq!(h.processed(), 0);
    }

    #[test]
    fn a_frame_processed_a_microsecond_after_its_exec_at_is_late() {
        let mut h = Harness::new();
        h.port.sys_time = Some(11 * US - 1);
        h.deliver(&scheduled(10));
        assert_eq!(h.telemetry().exec_late, 0);

        h.port.sys_time = Some(12 * US);
        h.deliver(&scheduled(11));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn a_frame_is_processed_and_late_when_the_device_time_is_unknown() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(10));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn exec_at_compares_across_the_u32_wraparound() {
        let mut h = Harness::new();
        let before_wrap = u64::from(u32::MAX) * US;
        h.port.sys_time = Some(before_wrap);
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 0);

        h.port.sys_time = Some(before_wrap + u64::from(LATER + 1) * US);
        assert!(h.process_one());
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);

        h.deliver(&scheduled(u32::MAX));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn clear_resets_exec_late() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(10));
        assert_eq!(h.telemetry().exec_late, 1);
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.telemetry().exec_late, 0);
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

    #[test]
    fn boot_brings_fpga_to_legacy_clear_baseline() {
        let h = Harness::new();

        assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);

        for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
            assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + u16::from(bank)), 1);
            assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + u16::from(bank)), 0xFFFF);
            assert_eq!(h.ctl(ADDR_MOD_REP0 + u16::from(bank)), REP_INFINITE);
            assert_eq!(h.mod_word(bank, 0), 0xFFFF);
        }

        for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
            assert_eq!(
                h.ctl(ADDR_PATTERN_MODE0 + u16::from(bank)),
                u16::from(EmissionType::Raw.as_u8())
            );
            assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0 + u16::from(bank)), 0);
            assert_eq!(h.ctl(ADDR_PATTERN_REP0 + u16::from(bank)), REP_INFINITE);
            assert_eq!(h.emission_word(bank, 0), 0);
            assert_eq!(h.emission_word(bank, NUM_TRANSDUCERS - 1), 0);
        }

        assert_eq!(h.port.phase_corr[0], 0);
        assert_eq!(h.port.phase_corr[PHASE_CORR_WORDS - 1], 0);
        assert_eq!(h.port.output_mask[0], 0xFFFF);
        assert_eq!(h.port.output_mask[OUTPUT_MASK_WORDS - 1], 0xFFFF);

        assert_eq!(h.port.pwe[0], 0x00);
        assert_eq!(h.port.pwe[1], 0x01);
        assert_eq!(h.port.pwe[128], 0x56);
        assert_eq!(h.port.pwe[PWE_TABLE_SIZE - 1], 0x100);

        assert_eq!(h.latch_count(CtlFlags::MOD_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::PATTERN_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::SILENCER_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::DEBUG_SET), 1);
        assert_eq!(h.ctl(ADDR_SILENCER_SET_RESULT), 0);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 0);
    }
}
