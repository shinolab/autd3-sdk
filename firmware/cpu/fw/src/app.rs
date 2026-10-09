use core::cell::Cell;
use core::sync::atomic::{AtomicU32, Ordering};

use enum_map::EnumMap;
use zerocopy::IntoBytes;

use autd3_cpu_wire::payload::FirmwareInfo;

use crate::cmd;
use crate::ctx::{IsrCell, MainCell};
use crate::fifo::{FIFO_DEPTH, Fifo};
use crate::fpga;
use crate::fpga_params::{
    ADDR_FPGA_STATE, ADDR_FUNCTION_BITS, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR,
    ADDR_VERSION_NUM_PATCH,
};
use crate::port::Port;
use crate::proto::{
    Cmd, Disposition, Drained, Error, FrameHeader, PAYLOAD_BYTES, Reply, ReplyData, RxFrame,
    Telemetry,
};
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

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
            header: Cell::new(FrameHeader { seq: 0, cmd: 0 }),
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

pub struct Cpu {
    last_seq: IsrCell<u8>,
    last_cmd: IsrCell<u8>,
    slots: [Slot; FIFO_DEPTH as usize],
    fifo: Fifo,
    telemetry: EnumMap<Telemetry, AtomicU32>,
    failsafe_fired: MainCell<bool>,
    ptp_unlock_failsafe_fired: MainCell<bool>,
    expected_seq: MainCell<u8>,
    pub(crate) config: MainCell<cmd::cpu_config::CpuConfig>,
    pub(crate) update: cmd::update::UpdateSession,
    pub(crate) fpga_update: cmd::fpga_update::FpgaUpdateSession,
    results: [Cell<Reply>; FIFO_DEPTH as usize],
}

const _: () = assert!(FIFO_DEPTH as usize > autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES);

type Outcome = Result<ReplyData, Error>;

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
            last_seq: IsrCell::new(0xFF),
            last_cmd: IsrCell::new(0xFF),
            slots: [const { Slot::new() }; FIFO_DEPTH as usize],
            fifo: Fifo::new(),
            telemetry: EnumMap::from_array([const { AtomicU32::new(0) }; Telemetry::ALL.len()]),
            failsafe_fired: MainCell::new(false),
            ptp_unlock_failsafe_fired: MainCell::new(false),
            expected_seq: MainCell::new(0),
            config: MainCell::new(cmd::cpu_config::default_config()),
            update: cmd::update::UpdateSession::new(),
            fpga_update: cmd::fpga_update::FpgaUpdateSession::new(),
            results: [const { Cell::new(Reply::RESET) }; FIFO_DEPTH as usize],
        }
    }
}

impl Cpu {
    pub fn init<P: Port>(&self, port: &mut P) {
        if fpga::init(port, self.config().fpga_wait_update_max_polls).is_err() {
            self.count_boot_failure();
        }
    }

    pub fn mark_boot_attempt<P: Port>(&self, port: &mut P) {
        if self.record_boot_attempt(port).is_err() {
            self.count_boot_failure();
        }
    }

    pub fn count_boot_failure(&self) {
        self.bump(Telemetry::BootFailure);
    }

    pub fn count_send_failures(&self, count: u32) {
        self.telemetry[Telemetry::SendFailure].fetch_add(count, Ordering::Relaxed);
    }

    #[must_use]
    pub fn booted_slot(&self) -> Option<autd3_cpu_wire::update::Slot> {
        self.update.boot_slot()
    }

    pub(crate) fn reset_telemetry(&self) {
        for (id, counter) in &self.telemetry {
            if id != Telemetry::BootFailure {
                counter.store(0, Ordering::Relaxed);
            }
        }
        self.failsafe_fired.set(false);
        self.ptp_unlock_failsafe_fired.set(false);
    }

    fn bump(&self, id: Telemetry) {
        self.telemetry[id].fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn telemetry(&self, id: Telemetry) -> u32 {
        self.telemetry[id].load(Ordering::Relaxed)
    }

    pub fn tick_1ms<P: Port>(&self, port: &mut P) {
        self.update_tick(port);
        self.fpga_update_tick(port);
        let config = self.config();
        let silent = cmd::failsafe::host_silent(port, &config);
        self.failsafe(port, silent, &self.failsafe_fired, Telemetry::Failsafe);
        let unlocked = cmd::failsafe::ptp_unlock_expired(port, &config);
        self.failsafe(
            port,
            unlocked,
            &self.ptp_unlock_failsafe_fired,
            Telemetry::PtpUnlockFailsafe,
        );
    }

    fn failsafe<P: Port>(
        &self,
        port: &mut P,
        tripped: bool,
        fired: &MainCell<bool>,
        counter: Telemetry,
    ) {
        if !tripped {
            fired.set(false);
            return;
        }
        if !fired.get() {
            fired.set(true);
            cmd::failsafe::mute(port);
            self.bump(counter);
        }
    }

    #[must_use]
    pub fn reply(&self) -> Reply {
        self.result_before(self.fifo.tail_acquire())
    }

    fn result_before(&self, tail: u16) -> Reply {
        self.results[Fifo::slot(tail.wrapping_sub(1))].get()
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn expected_seq(&self) -> u8 {
        self.expected_seq.get()
    }

    pub fn recv_frame(&self, frame: &[u8], msg_id: u16) -> Disposition {
        let Some((header, payload)) = FrameHeader::parse(frame) else {
            return Disposition::Dropped;
        };
        let FrameHeader { seq, cmd: raw_cmd } = *header;
        if seq == self.last_seq.get() && raw_cmd == self.last_cmd.get() {
            self.bump(Telemetry::Dedup);
            let completed_ack = if Cmd::from_u8(raw_cmd) == Some(Cmd::Reset) {
                Reply::RESET.ack
            } else {
                seq
            };
            return if self.reply().ack == completed_ack {
                Disposition::Reply
            } else {
                Disposition::Dropped
            };
        }

        let head = self.fifo.head();
        if Fifo::is_full(head, self.fifo.tail_acquire()) {
            self.bump(Telemetry::FifoDrop);
            return Disposition::Dropped;
        }
        self.slots[Fifo::slot(head)].store(*header, payload, msg_id);
        self.fifo.publish(head);
        self.last_seq.set(seq);
        self.last_cmd.set(raw_cmd);
        Disposition::Deferred
    }

    pub fn process_one<P: Port>(&self, port: &mut P, work: &mut RxFrame) -> Drained {
        let Some(tail) = self.fifo.next() else {
            return Drained::Empty;
        };
        self.slots[Fifo::slot(tail)].load(work);
        let result = self
            .handle_frame(port, work)
            .unwrap_or_else(|| self.result_before(tail));
        self.results[Fifo::slot(tail)].set(result);
        self.fifo.commit(tail);
        Drained::Completed {
            msg_id: work.msg_id,
        }
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

    fn handle_frame<P: Port>(&self, port: &mut P, in_frame: &RxFrame) -> Option<Reply> {
        let cmd = Cmd::from_u8(in_frame.header.cmd);
        if cmd == Some(Cmd::Reset) {
            self.expected_seq.set(0);
            return Some(Reply::RESET);
        }
        if in_frame.header.seq != self.expected_seq.get() {
            self.bump(Telemetry::SeqMismatch);
            return None;
        }
        self.expected_seq.set(in_frame.header.seq.wrapping_add(1));
        let outcome = match cmd {
            Some(cmd) => self.dispatch(port, cmd, in_frame.payload()),
            None => Err(Error::UnknownCmd),
        };
        let result = match outcome {
            Ok(data) => Reply::new(in_frame.header.seq, Error::None, data),
            Err(err) => {
                self.bump(Telemetry::DispatchError);
                Reply::new(in_frame.header.seq, err, ReplyData::EMPTY)
            }
        };
        self.bump(Telemetry::Processed);
        Some(result)
    }

    fn read_telemetry<P: Port>(&self, port: &mut P) -> ReplyData {
        let mut bytes = [0u8; Telemetry::REPLY_BYTES];
        for (chunk, id) in bytes
            .chunks_mut(Telemetry::COUNTER_BYTES)
            .zip(Telemetry::ALL)
        {
            let value = match id {
                Telemetry::SyncResync => u32::from(fpga::read_ctl(port, ADDR_FPGA_STATE) >> 8),
                id => self.telemetry(*id),
            };
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        ReplyData::from_slice(&bytes)
    }

    fn read_firmware_info<P: Port>(&self, port: &mut P) -> ReplyData {
        let info = FirmwareInfo {
            cpu_version: [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH],
            fpga_version: [
                fpga::read_ctl(port, ADDR_VERSION_NUM_MAJOR) as u8,
                fpga::read_ctl(port, ADDR_VERSION_NUM_MINOR) as u8,
                fpga::read_ctl(port, ADDR_VERSION_NUM_PATCH) as u8,
            ],
            fpga_functions: fpga::read_ctl(port, ADDR_FUNCTION_BITS) as u8,
            fpga_boot_image: self.fpga_boot_image(port) as u8,
        };
        ReplyData::from_slice(info.as_bytes())
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
            Cmd::ReadFpgaState => {
                return Ok(ReplyData::from_slice(&[
                    fpga::read_ctl(port, ADDR_FPGA_STATE) as u8,
                ]));
            }
            Cmd::ReadTelemetry => return Ok(self.read_telemetry(port)),
            Cmd::ReadFirmwareInfo => return Ok(self.read_firmware_info(port)),
            Cmd::ReadRunningImage => {
                return self
                    .running_image(port)
                    .map(|image| ReplyData::from_slice(&[image.as_u8()]));
            }
            Cmd::WriteFociBuffer => cmd::write_foci::handle(port, payload),
            Cmd::WritePatternRaw => cmd::write_pattern_raw::handle(port, payload),
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
            Cmd::ReleaseFailsafe => cmd::failsafe::release(port, &config),
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
            _ => Err(Error::UnknownCmd),
        };
        result.map(|()| ReplyData::EMPTY)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {

    use crate::cmd::config_mod::ConfigModPayload;
    use crate::cmd::force_fan::ForceFanPayload;
    use crate::fifo::FIFO_DEPTH;
    use crate::fpga::{PHASE_CORR_WORDS, PWE_TABLE_SIZE, REP_INFINITE};
    use crate::fpga_params::{
        ADDR_FPGA_STATE, ADDR_FUNCTION_BITS, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0,
        ADDR_PATTERN_CYCLE0, ADDR_PATTERN_MODE0, ADDR_PATTERN_REP0,
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        ADDR_SILENCER_FLAG, ADDR_SILENCER_SET_RESULT, ADDR_SILENCER_UPDATE_RATE_INTENSITY,
        ADDR_SILENCER_UPDATE_RATE_PHASE, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR,
        ADDR_VERSION_NUM_PATCH, CtlFlags, EmissionType, NUM_BANKS, NUM_TRANSDUCERS,
    };
    use crate::proto::{
        Cmd, Disposition, Drained, Error, FRAME_BYTES_MAX, OUTPUT_MASK_WORDS, RxFrame, Telemetry,
    };
    use crate::test_utils::builders::{config_mod, force_fan, write_foci_buffer, write_mod_buffer};
    use crate::test_utils::mock::{Frame, Harness};
    use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

    #[test]
    fn initial_ack_is_sentinel_byte() {
        let h = Harness::new();
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);
    }

    #[test]
    fn a_clean_boot_counts_no_boot_failure() {
        let mut h = Harness::new();
        h.reboot();
        assert_eq!(h.telemetry(Telemetry::BootFailure), 0);
    }

    #[test]
    fn an_fpga_timeout_at_boot_is_counted_and_survives_clear() {
        let mut h = Harness::new();
        h.port.latch_stuck = true;
        h.reboot();
        h.port.latch_stuck = false;
        assert_eq!(h.telemetry(Telemetry::BootFailure), 1);

        h.deliver(&Frame::new(0, Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry(Telemetry::BootFailure), 1);
    }

    #[test]
    fn a_flash_failure_while_marking_the_boot_is_counted() {
        let mut h = Harness::new();
        h.port.flash_fail = true;
        h.reboot();
        h.port.flash_fail = false;
        assert_eq!(h.telemetry(Telemetry::BootFailure), 1);
    }

    #[test]
    fn boot_failures_reported_by_the_board_accumulate() {
        let h = Harness::new();
        h.cpu.count_boot_failure();
        h.cpu.count_boot_failure();
        assert_eq!(h.telemetry(Telemetry::BootFailure), 2);
    }

    #[test]
    fn send_failures_are_counted_and_cleared_by_clear() {
        let mut h = Harness::new();
        h.cpu.count_send_failures(2);
        h.cpu.count_send_failures(1);
        assert_eq!(h.telemetry(Telemetry::SendFailure), 3);

        h.deliver(&Frame::new(0, Cmd::Clear));
        assert_eq!(h.telemetry(Telemetry::SendFailure), 0);
    }

    #[test]
    fn matching_seq_advances_ack_and_expected_seq() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
        assert_eq!(h.status(), Error::None);

        h.deliver(&Frame::new(1, Cmd::ReadFirmwareInfo));
        assert_eq!(h.ack(), 1);
        assert_eq!(h.expected_seq(), 2);
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.firmware_info().cpu_version,
            [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
        );
    }

    #[test]
    fn mismatched_seq_is_dropped() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(5, Cmd::Nop));
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);
    }

    #[test]
    fn unknown_cmd_reports_unknown_cmd_and_advances_seq() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0, 0x7F));
        assert_eq!(h.status(), Error::UnknownCmd);
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
        assert_eq!(h.telemetry(Telemetry::DispatchError), 1);
    }

    #[test]
    fn every_cmd_has_a_dispatch_arm() {
        for &cmd in Cmd::ALL {
            let mut h = Harness::new();
            h.deliver(&Frame::new(0, cmd));
            assert_ne!(h.status(), Error::UnknownCmd, "{cmd:?}");
        }
    }

    #[test]
    fn duplicate_frame_is_suppressed_at_isr_boundary() {
        let mut h = Harness::new();
        let f = Frame::new(0, Cmd::Nop);
        h.deliver(&f);
        h.deliver(&f);
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn reset_arriving_during_a_dispatch_applies_after_that_frame() {
        let mut h = Harness::new();

        let stale = write_foci_buffer(0, 0, 0, &[0x5A5A]);
        h.deliver_no_drain(&stale);
        h.arm_isr_frame(0, Cmd::Reset);

        assert!(h.process_one());
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);

        assert!(h.process_one());
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 0);

        assert!(!h.process_one());

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn reset_returns_proto_state_to_post_boot_baseline() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(0, Cmd::Nop));
        h.deliver(&Frame::new(1, Cmd::Nop));
        assert_eq!(h.ack(), 1);
        assert_eq!(h.expected_seq(), 2);

        h.deliver(&Frame::new(99, Cmd::Reset));
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);

        h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().cpu_version,
            [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
        );
    }

    #[test]
    fn nop_acks_without_changing_state() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 1);
        assert_eq!(h.telemetry(Telemetry::DispatchError), 0);
    }

    #[test]
    fn seq_wraparound_boundary() {
        let mut h = Harness::new();
        for i in 0..257u16 {
            h.deliver(&Frame::new((i & 0xFF) as u8, Cmd::Nop));
        }
        assert_eq!(h.expected_seq(), 1);
        assert_eq!(h.ack(), 0);
    }

    #[test]
    fn unknown_non_streaming_cmd_reports_unknown_cmd() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0, 0xEE));
        assert_eq!(h.status(), Error::UnknownCmd);
    }

    #[test]
    fn consecutive_frames_each_process_immediately() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        h.deliver(&Frame::new(1, Cmd::Nop));
        assert_eq!(h.ack(), 1);
        h.deliver(&Frame::new(2, Cmd::Nop));
        assert_eq!(h.ack(), 2);
        assert_eq!(h.expected_seq(), 3);
    }

    #[test]
    fn same_seq_different_cmd_is_not_suppressed_at_isr_boundary() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Reset));
        assert_eq!(h.expected_seq(), 0);

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn dedup_state_resets_on_reboot() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.expected_seq(), 1);

        h.reboot();
        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn handshake_survives_worst_case_dedup_collision_after_crashed_client() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Reset));

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.expected_seq(), 1);

        h.deliver(&Frame::new(0, Cmd::Reset));
        h.deliver(&Frame::new(1, Cmd::Reset));

        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);

        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn fixed_length_payloads_must_match_exactly() {
        let mut h = Harness::new();
        let mut long = force_fan(0, 1);
        long.set_payload_byte(size_of::<ForceFanPayload>(), 0);
        h.deliver(&long);
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut short = config_mod(1, 0, 10, 4);
        short.set_len(size_of::<ConfigModPayload>() - 1);
        h.deliver(&short);
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&force_fan(2, 1));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn an_oversized_frame_is_dropped() {
        let h = Harness::new();
        let mut frame = std::vec![0u8; FRAME_BYTES_MAX + 1];
        frame[1] = Cmd::Nop as u8;
        assert_eq!(h.cpu.recv_frame(&frame, 0), Disposition::Dropped);
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);
    }

    #[test]
    fn fifo_mode_defers_processing_until_drained() {
        let mut h = Harness::new();

        h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
    }

    #[test]
    fn fifo_mode_drains_in_order() {
        let mut h = Harness::new();

        h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
        h.deliver_no_drain(&Frame::new(1, Cmd::Nop));
        h.deliver_no_drain(&Frame::new(2, Cmd::Nop));
        assert_eq!(h.expected_seq(), 0);

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.ack(), 2);
        assert_eq!(h.expected_seq(), 3);
    }

    #[test]
    fn reset_is_deferred_until_drained() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));

        h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);

        assert!(h.process_one());
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 0);
    }

    #[test]
    fn frames_queued_before_a_reset_are_processed_before_it() {
        let mut h = Harness::new();

        h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
        h.deliver_no_drain(&Frame::new(1, Cmd::Nop));
        h.deliver_no_drain(&Frame::new(2, Cmd::Nop));

        assert!(h.process_one());
        assert_eq!(h.ack(), 0);

        h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);

        assert!(h.process_one());
        assert!(h.process_one());
        assert_eq!(h.ack(), 2);
        assert_eq!(h.expected_seq(), 3);

        assert!(h.process_one());
        assert_eq!(h.ack(), 0xFF);
        assert_eq!(h.expected_seq(), 0);
        assert!(!h.process_one());
        assert_eq!(h.telemetry(Telemetry::Processed), 3);
    }

    #[test]
    fn a_frame_queued_after_a_reset_is_accepted_in_the_new_seq_space() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        h.deliver(&Frame::new(1, Cmd::Nop));

        h.deliver_no_drain(&Frame::new(2, Cmd::Nop));
        h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
        h.deliver_no_drain(&Frame::new(0, Cmd::Nop));

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.ack(), 0);
        assert_eq!(h.expected_seq(), 1);
        assert_eq!(h.telemetry(Telemetry::SeqMismatch), 0);
    }

    #[test]
    fn a_seq_mismatch_keeps_the_previous_result() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
        let before = h.cpu.reply();
        assert_ne!(before.data(), []);

        h.deliver(&Frame::new(9, Cmd::Nop));
        assert_eq!(h.telemetry(Telemetry::SeqMismatch), 1);
        assert_eq!(h.cpu.reply(), before);
    }

    #[test]
    fn the_reply_follows_every_completed_frame_across_the_ring_wrap() {
        let mut h = Harness::new();
        for seq in 0..(3 * FIFO_DEPTH as u8) {
            h.deliver(&Frame::new(seq, Cmd::Nop));
            assert_eq!(h.ack(), seq);
        }
    }

    #[test]
    fn the_reply_is_reset_ack_until_the_first_frame_completes() {
        let mut h = Harness::new();
        for seq in 0..=(FIFO_DEPTH as u8) {
            h.deliver(&Frame::new(seq, Cmd::Nop));
        }
        h.reboot();
        assert_eq!(h.cpu.reply(), crate::proto::Reply::RESET);
    }

    #[test]
    fn the_isr_sees_the_previous_result_while_a_frame_is_being_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
        let previous = h.cpu.reply();

        h.deliver_no_drain(&crate::test_utils::builders::write_foci_buffer(
            1,
            0,
            0,
            &[0x5A5A],
        ));
        h.arm_isr_frame(2, Cmd::Nop);
        assert!(h.process_one());

        assert_eq!(h.port.isr_seen_reply, Some(previous));
        assert_eq!(h.ack(), 1);
    }

    #[test]
    fn fifo_overflow_drops_beyond_capacity_and_accepts_after_drain() {
        let mut h = Harness::new();

        let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
        for i in 0..capacity {
            h.deliver_no_drain(&Frame::new(i, Cmd::Nop));
        }
        h.deliver_no_drain(&Frame::new(capacity, Cmd::Nop));

        h.cpu.process_pending(&mut h.port);
        assert_eq!(h.ack(), capacity - 1);
        assert_eq!(h.expected_seq(), capacity);

        h.deliver(&Frame::new(capacity, Cmd::Nop));
        assert_eq!(h.ack(), capacity);
        assert_eq!(h.expected_seq(), capacity + 1);
    }

    #[test]
    fn a_fifo_frame_completes_with_its_msg_id() {
        let h = Harness::new();
        let frame = Frame::new(0, Cmd::Nop).bytes();
        assert_eq!(
            h.cpu.recv_frame(&frame[..2], 0x1234),
            crate::proto::Disposition::Deferred
        );
        let mut port = crate::test_utils::mock::MockPort::new();
        let mut work = RxFrame::ZERO;
        assert_eq!(
            h.cpu.process_one(&mut port, &mut work),
            Drained::Completed { msg_id: 0x1234 }
        );
        assert_eq!(h.cpu.process_one(&mut port, &mut work), Drained::Empty);
        assert_eq!(h.cpu.reply().ack, 0);
    }

    #[test]
    fn a_step_completes_one_frame_per_call_with_its_msg_id() {
        let h = Harness::new();
        let mut port = crate::test_utils::mock::MockPort::new();
        let mut work = RxFrame::ZERO;
        let _ = h.cpu.recv_frame(&[0, Cmd::Nop as u8], 0x11);
        let _ = h.cpu.recv_frame(&[1, Cmd::Nop as u8], 0x22);
        assert_eq!(
            h.cpu.step(&mut port, &mut work, 0),
            Drained::Completed { msg_id: 0x11 }
        );
        assert_eq!(h.cpu.reply().ack, 0);
        assert_eq!(
            h.cpu.step(&mut port, &mut work, 3),
            Drained::Completed { msg_id: 0x22 }
        );
        assert_eq!(h.cpu.reply().ack, 1);
        assert_eq!(h.cpu.step(&mut port, &mut work, 0), Drained::Empty);
    }

    #[test]
    fn a_duplicate_is_answered_by_the_isr_and_a_reset_completes_with_its_msg_id() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Nop as u8], 1),
            crate::proto::Disposition::Reply
        );
        assert_eq!(
            h.cpu.recv_frame(&[7, Cmd::Reset as u8], 2),
            crate::proto::Disposition::Deferred
        );
        assert!(h.process_one());
        assert_eq!(h.ack(), 0xFF);
    }

    #[test]
    fn a_duplicate_of_a_frame_still_in_the_fifo_gets_no_reply() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(
            h.cpu.recv_frame(&[1, Cmd::Nop as u8], 1),
            crate::proto::Disposition::Deferred
        );
        assert_eq!(h.ack(), 0);
        assert_eq!(
            h.cpu.recv_frame(&[1, Cmd::Nop as u8], 1),
            crate::proto::Disposition::Dropped
        );
        assert_eq!(h.telemetry(Telemetry::Dedup), 1);

        h.cpu.process_pending(&mut h.port);
        assert_eq!(
            h.cpu.recv_frame(&[1, Cmd::Nop as u8], 2),
            crate::proto::Disposition::Reply
        );
        assert_eq!(h.ack(), 1);
        assert_eq!(h.telemetry(Telemetry::Dedup), 2);
        assert_eq!(h.telemetry(Telemetry::Processed), 2);
    }

    #[test]
    fn a_retransmitted_reset_is_answered_only_after_it_is_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Reset as u8], 1),
            crate::proto::Disposition::Deferred
        );
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Reset as u8], 2),
            crate::proto::Disposition::Dropped
        );
        assert_eq!(h.ack(), 0);

        h.cpu.process_pending(&mut h.port);
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Reset as u8], 3),
            crate::proto::Disposition::Reply
        );
        assert_eq!(h.ack(), 0xFF);
    }

    #[test]
    fn a_duplicate_of_a_frame_rejected_for_its_seq_gets_no_reply() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        h.deliver(&Frame::new(5, Cmd::Nop));
        assert_eq!(h.ack(), 0);
        assert_eq!(
            h.cpu.recv_frame(&[5, Cmd::Nop as u8], 1),
            crate::proto::Disposition::Dropped
        );
        assert_eq!(h.telemetry(Telemetry::SeqMismatch), 1);
        assert_eq!(h.telemetry(Telemetry::Dedup), 1);
    }

    #[test]
    fn repeated_resets_are_each_answered_with_the_reset_ack() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Reset));
        for msg_id in 1..4 {
            assert_eq!(
                h.cpu.recv_frame(&[0, Cmd::Reset as u8], msg_id),
                crate::proto::Disposition::Reply
            );
            assert_eq!(h.ack(), 0xFF);
        }
        assert_eq!(h.expected_seq(), 0);
        assert_eq!(h.telemetry(Telemetry::Dedup), 3);
    }

    #[test]
    fn a_full_fifo_drops_the_frame_without_a_reply() {
        let h = Harness::new();
        let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
        for seq in 0..capacity {
            assert_eq!(
                h.cpu.recv_frame(&[seq, Cmd::Nop as u8], 0),
                crate::proto::Disposition::Deferred
            );
        }
        assert_eq!(
            h.cpu.recv_frame(&[capacity, Cmd::Nop as u8], 0),
            crate::proto::Disposition::Dropped
        );
        assert_eq!(
            h.cpu.recv_frame(&[0, Cmd::Reset as u8], 0),
            crate::proto::Disposition::Dropped
        );
    }

    fn read_telemetry(h: &mut Harness, seq: u8) -> std::vec::Vec<u32> {
        h.deliver(&Frame::new(seq, Cmd::ReadTelemetry));
        assert_eq!(h.status(), Error::None);
        let data = h.reply_data();
        assert_eq!(data.len(), Telemetry::REPLY_BYTES);
        data.chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn telemetry_counts_processed_frames() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, Cmd::Nop));
        h.deliver(&Frame::new(1, Cmd::Nop));
        assert_eq!(h.telemetry(Telemetry::Processed), 2);
    }

    #[test]
    fn telemetry_counts_dedup_hits() {
        let mut h = Harness::new();
        let f = Frame::new(0, Cmd::Nop);
        h.deliver(&f);
        h.deliver(&f);
        assert_eq!(h.telemetry(Telemetry::Dedup), 1);
        assert_eq!(h.telemetry(Telemetry::Processed), 1);
    }

    #[test]
    fn telemetry_counts_seq_mismatch() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(5, Cmd::Nop));
        assert_eq!(h.telemetry(Telemetry::SeqMismatch), 1);
        assert_eq!(h.telemetry(Telemetry::Processed), 0);
    }

    #[test]
    fn telemetry_counts_dispatch_errors() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 2));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.telemetry(Telemetry::DispatchError), 1);
    }

    #[test]
    fn telemetry_counts_fifo_drops() {
        let mut h = Harness::new();

        let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
        for i in 0..=capacity {
            h.deliver_no_drain(&Frame::new(i, Cmd::Nop));
        }
        assert_eq!(h.telemetry(Telemetry::FifoDrop), 1);
    }

    #[test]
    fn read_telemetry_returns_every_counter_at_once() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 2));

        let counters = read_telemetry(&mut h, 1);
        assert_eq!(counters.len(), Telemetry::ALL.len());
        assert_eq!(counters[Telemetry::DispatchError as usize], 1);
        assert_eq!(counters[Telemetry::Processed as usize], 1);
        assert_eq!(counters[Telemetry::FifoDrop as usize], 0);
    }

    #[test]
    fn read_telemetry_sync_resync_returns_fpga_state_high_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
        let counters = read_telemetry(&mut h, 0);
        assert_eq!(counters[Telemetry::SyncResync as usize], 0x2A);
    }

    #[test]
    fn telemetry_counters_are_wider_than_a_byte() {
        let mut h = Harness::new();
        let mut seq = 0u8;
        for _ in 0..300 {
            h.deliver(&Frame::new(seq, Cmd::Nop));
            seq = seq.wrapping_add(1);
        }
        let counters = read_telemetry(&mut h, seq);
        assert_eq!(counters[Telemetry::Processed as usize], 300);
    }

    #[test]
    fn derived_telemetry_id_is_not_a_cpu_counter() {
        let h = Harness::new();
        for &id in Telemetry::ALL {
            assert_eq!(h.telemetry(id), 0);
        }
        assert_eq!(h.telemetry(Telemetry::SyncResync), 0);
    }

    #[test]
    fn read_fpga_functions_returns_function_bits_register() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FUNCTION_BITS, 0xA5);

        h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
        assert_eq!(h.firmware_info().fpga_functions, 0xA5);
    }

    #[test]
    fn fpga_state_survives_reset() {
        let mut h = Harness::new();

        h.deliver(&write_foci_buffer(0, 0, 0, &[0x5A5A]));
        h.deliver(&write_mod_buffer(1, 1, 8, &[0x77]));
        h.deliver(&config_mod(2, 1, 5, 256));
        assert_eq!(h.status(), Error::None);

        h.deliver(&Frame::new(99, Cmd::Reset));
        assert_eq!(h.expected_seq(), 0);

        assert_eq!(h.emission_word(0, 0), 0x5A5A);
        assert_eq!(h.mod_word(1, 4), 0x0077);
        assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 255);
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

    #[test]
    fn read_fpga_state_returns_register_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x83);
        h.deliver(&Frame::new(0, Cmd::ReadFpgaState));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.reply_data(), [0x83]);
    }

    #[test]
    fn read_fpga_fw_version_returns_register_bytes() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0x0A);
        h.set_ctl(ADDR_VERSION_NUM_MINOR, 0x0B);
        h.set_ctl(ADDR_VERSION_NUM_PATCH, 0x0C);

        h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.firmware_info().fpga_version, [0x0A, 0x0B, 0x0C]);
    }
}
