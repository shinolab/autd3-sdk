use core::cell::Cell;
use core::mem::offset_of;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::cmd;
use crate::fpga;
use crate::params::{
    ADDR_FPGA_STATE, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH,
    BRAM_SELECT_CONTROLLER,
};
use crate::port::Port;
use crate::proto::{
    AL_STATUS_CODE_SM_WATCHDOG, AL_STATUS_CODE_SYNC_ERROR, Cmd, Error, FAILSAFE_TICKS, Mode,
    ProtoState, RxFrame, Telemetry, TxFrame, WIRE_RX_FRAME_BYTES,
};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ReadTelemetryPayload {
    pub counter_id: u8,
}

const _: () = assert!(offset_of!(ReadTelemetryPayload, counter_id) == 0);

pub const FIFO_DEPTH: u16 = 8;
const FIFO_MASK: u16 = FIFO_DEPTH - 1;
const FIFO_CAPACITY: u16 = FIFO_DEPTH - 1;

pub struct Cpu {
    mode: AtomicU8,
    last_seq: AtomicU8,
    last_cmd: AtomicU8,
    fifo: [Cell<RxFrame>; FIFO_DEPTH as usize],
    fifo_head: AtomicU16,
    fifo_tail: AtomicU16,
    fifo_flush_head: AtomicU16,
    fifo_flush_gen: AtomicU16,
    fifo_flush_seen: AtomicU16,
    preempt_tx: AtomicU16,
    preempt_expected: AtomicU8,
    preempt_mute: AtomicBool,
    telemetry: [AtomicU8; Telemetry::COUNT],
    al_err_ticks: Cell<u16>,
    proto: ProtoState,
    pub(crate) silencer: cmd::silencer::SilencerGuard,
    tx: AtomicU16,
}

fn pack_tx(ack: u8, data: u8) -> u16 {
    u16::from(ack) | (u16::from(data) << 8)
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mode: AtomicU8::new(Mode::Fifo as u8),
            last_seq: AtomicU8::new(0xFF),
            last_cmd: AtomicU8::new(0xFF),
            fifo: [const { Cell::new(RxFrame::ZERO) }; FIFO_DEPTH as usize],
            fifo_head: AtomicU16::new(0),
            fifo_tail: AtomicU16::new(0),
            fifo_flush_head: AtomicU16::new(0),
            fifo_flush_gen: AtomicU16::new(0),
            fifo_flush_seen: AtomicU16::new(0),
            preempt_tx: AtomicU16::new(0),
            preempt_expected: AtomicU8::new(0),
            preempt_mute: AtomicBool::new(false),
            telemetry: [const { AtomicU8::new(0) }; Telemetry::COUNT],
            al_err_ticks: Cell::new(0),
            proto: ProtoState::new(),
            silencer: cmd::silencer::SilencerGuard::new(),
            tx: AtomicU16::new(0),
        }
    }

    pub fn init<P: Port>(&self, port: &mut P) {
        self.set_mode(Mode::Fifo);
        self.proto.init();
        if let Err(err) = fpga::init(port, self.mode()) {
            self.proto.error_detail.set(Some(err));
        }
        self.silencer.init();
        self.reset_telemetry();
        self.tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
        self.last_seq.store(0xFF, Ordering::Relaxed);
        self.last_cmd.store(0xFF, Ordering::Relaxed);
        self.fifo_head.store(0, Ordering::Relaxed);
        self.fifo_tail.store(0, Ordering::Relaxed);
        self.fifo_flush_head.store(0, Ordering::Relaxed);
        self.fifo_flush_gen.store(0, Ordering::Relaxed);
        self.fifo_flush_seen.store(0, Ordering::Relaxed);
        self.preempt_tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
        self.preempt_expected.store(0, Ordering::Relaxed);
        self.preempt_mute.store(false, Ordering::Relaxed);
    }

    pub(crate) fn reset_telemetry(&self) {
        for counter in &self.telemetry {
            counter.store(0, Ordering::Relaxed);
        }
        self.al_err_ticks.set(0);
    }

    fn bump(&self, id: Telemetry) {
        self.telemetry[id as usize].fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn telemetry(&self, id: Telemetry) -> u8 {
        self.telemetry[id as usize].load(Ordering::Relaxed)
    }

    pub fn tick_1ms<P: Port>(&self, port: &mut P) {
        let code = port.al_status_code();
        if code != AL_STATUS_CODE_SYNC_ERROR && code != AL_STATUS_CODE_SM_WATCHDOG {
            self.al_err_ticks.set(0);
            return;
        }
        let ticks = self.al_err_ticks.get().saturating_add(1);
        self.al_err_ticks.set(ticks);
        if ticks == FAILSAFE_TICKS {
            cmd::stop::mute(port);
            self.bump(Telemetry::Failsafe);
        }
    }

    #[must_use]
    pub fn tx(&self) -> TxFrame {
        let packed = self.tx.load(Ordering::Relaxed);
        TxFrame {
            ack: (packed & 0xFF) as u8,
            data: (packed >> 8) as u8,
        }
    }

    #[cfg(test)]
    pub(crate) fn expected_seq(&self) -> u8 {
        self.proto.expected_seq.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn set_fw_version(&self, major: u8, minor: u8, patch: u8) {
        self.proto.fw_version_major.set(major);
        self.proto.fw_version_minor.set(minor);
        self.proto.fw_version_patch.set(patch);
    }

    #[cfg(test)]
    pub(crate) fn set_error_detail(&self, err: Error) {
        self.proto.error_detail.set(Some(err));
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        Mode::from_u8(self.mode.load(Ordering::Relaxed)).unwrap_or(Mode::Fifo)
    }

    pub(crate) fn set_mode(&self, mode: Mode) {
        self.mode.store(mode as u8, Ordering::Relaxed);
    }

    pub fn recv_ethercat<P: Port>(&self, port: &mut P, frame: &[u8; WIRE_RX_FRAME_BYTES]) {
        let seq = frame[0];
        let raw_cmd = frame[1];
        if seq == self.last_seq.load(Ordering::Relaxed)
            && raw_cmd == self.last_cmd.load(Ordering::Relaxed)
        {
            self.bump(Telemetry::Dedup);
            return;
        }

        let head = self.fifo_head.load(Ordering::Relaxed);
        let cmd = Cmd::from_u8(raw_cmd);
        let preempt = matches!(cmd, Some(Cmd::Reset | Cmd::Stop));
        if preempt {
            if cmd == Some(Cmd::Reset) {
                self.preempt_tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
                self.preempt_expected.store(0, Ordering::Relaxed);
                self.preempt_mute.store(false, Ordering::Relaxed);
            } else {
                self.preempt_tx.store(pack_tx(seq, 0), Ordering::Relaxed);
                self.preempt_expected
                    .store(seq.wrapping_add(1), Ordering::Relaxed);
                self.preempt_mute.store(true, Ordering::Relaxed);
            }
            self.fifo_flush_head.store(head, Ordering::Relaxed);
            self.fifo_flush_gen.store(
                self.fifo_flush_gen.load(Ordering::Relaxed).wrapping_add(1),
                Ordering::Release,
            );
        }

        let tail = self.fifo_tail.load(Ordering::Acquire);
        let inline_ok = preempt || (self.mode() == Mode::LowLatency && tail == head);
        if inline_ok {
            self.handle_frame(port, &RxFrame::from_wire(frame));
            self.last_seq.store(seq, Ordering::Relaxed);
            self.last_cmd.store(raw_cmd, Ordering::Relaxed);
            return;
        }

        if head.wrapping_sub(tail) >= FIFO_CAPACITY {
            self.bump(Telemetry::FifoDrop);
            return;
        }
        self.fifo[(head & FIFO_MASK) as usize].set(RxFrame::from_wire(frame));
        self.fifo_head
            .store(head.wrapping_add(1), Ordering::Release);
        self.last_seq.store(seq, Ordering::Relaxed);
        self.last_cmd.store(raw_cmd, Ordering::Relaxed);
    }

    pub fn process_one<P: Port>(&self, port: &mut P) -> bool {
        let flush_gen = self.fifo_flush_gen.load(Ordering::Acquire);
        if flush_gen != self.fifo_flush_seen.load(Ordering::Relaxed) {
            self.fifo_flush_seen.store(flush_gen, Ordering::Relaxed);
            let flush_head = self.fifo_flush_head.load(Ordering::Relaxed);
            if flush_head.wrapping_sub(self.fifo_tail.load(Ordering::Relaxed)) < FIFO_DEPTH {
                self.fifo_tail.store(flush_head, Ordering::Release);
            }
        }
        let tail = self.fifo_tail.load(Ordering::Relaxed);
        if tail == self.fifo_head.load(Ordering::Acquire) {
            return false;
        }
        let in_frame = self.fifo[(tail & FIFO_MASK) as usize].get();
        self.handle_frame(port, &in_frame);
        self.fifo_tail
            .store(tail.wrapping_add(1), Ordering::Release);
        if self.fifo_flush_gen.load(Ordering::Acquire) != flush_gen {
            self.apply_preempt(port);
        }
        true
    }

    pub fn process_pending<P: Port>(&self, port: &mut P) {
        while self.process_one(port) {}
    }

    fn apply_preempt<P: Port>(&self, port: &mut P) {
        if self.preempt_mute.load(Ordering::Relaxed) {
            cmd::stop::mute(port);
        }
        self.proto.expected_seq.store(
            self.preempt_expected.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.tx
            .store(self.preempt_tx.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    fn handle_frame<P: Port>(&self, port: &mut P, in_frame: &RxFrame) {
        let cmd = Cmd::from_u8(in_frame.cmd);
        if matches!(cmd, Some(Cmd::Reset | Cmd::Stop)) {
            self.apply_preempt(port);
            return;
        }
        if in_frame.seq == self.proto.expected_seq.load(Ordering::Relaxed) {
            self.proto
                .expected_seq
                .store(in_frame.seq.wrapping_add(1), Ordering::Relaxed);
            let data = match cmd {
                Some(cmd) => self.dispatch(port, cmd, &in_frame.payload),
                None => self.latch_error(Error::UnknownCmd),
            };
            self.tx
                .store(pack_tx(in_frame.seq, data), Ordering::Relaxed);
            self.bump(Telemetry::Processed);
        } else {
            self.bump(Telemetry::SeqMismatch);
        }
    }

    fn latch_error(&self, err: Error) -> u8 {
        self.proto.error_detail.set(Some(err));
        self.bump(Telemetry::DispatchError);
        err as u8
    }

    fn read_telemetry(&self, payload: &[u8]) -> u8 {
        let Ok((p, _)) = ReadTelemetryPayload::ref_from_prefix(payload) else {
            return self.latch_error(Error::InvalidPayload);
        };
        match Telemetry::from_u8(p.counter_id) {
            Some(id) => self.telemetry(id),
            None => self.latch_error(Error::InvalidPayload),
        }
    }

    fn dispatch<P: Port>(&self, port: &mut P, cmd: Cmd, payload: &[u8]) -> u8 {
        let result = match cmd {
            Cmd::Reset | Cmd::Stop | Cmd::Nop => Ok(()),
            Cmd::XorHash => cmd::xor_hash::handle(port, payload),
            Cmd::ReadCpuFwVersionMajor => return self.proto.fw_version_major.get(),
            Cmd::ReadCpuFwVersionMinor => return self.proto.fw_version_minor.get(),
            Cmd::ReadCpuFwVersionPatch => return self.proto.fw_version_patch.get(),
            Cmd::ReadFpgaFwVersionMajor => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) as u8;
            }
            Cmd::ReadFpgaFwVersionMinor => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MINOR) as u8;
            }
            Cmd::ReadFpgaFwVersionPatch => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_PATCH) as u8;
            }
            Cmd::ReadErrorDetail => {
                return self.proto.error_detail.get().map_or(0, |err| err as u8);
            }
            Cmd::ReadFpgaState => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_FPGA_STATE) as u8;
            }
            Cmd::ReadTelemetry => return self.read_telemetry(payload),
            Cmd::ReadFpgaFunctions => {
                return (fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) >> 8)
                    as u8;
            }
            Cmd::WritePatternBuffer => cmd::write_pattern::handle(port, payload),
            Cmd::WritePatternCompressed => cmd::write_pattern_compressed::handle(port, payload),
            Cmd::WriteModBuffer => cmd::write_mod::handle(port, payload),
            Cmd::ConfigMod => self.config_mod(port, payload),
            Cmd::ConfigPattern => self.config_pattern(port, payload),
            Cmd::ChangeModBank => self.change_mod_bank(port, payload),
            Cmd::ChangePatternBank => self.change_pattern_bank(port, payload),
            Cmd::SetSilencer => self.set_silencer(port, payload),
            Cmd::SetPhaseCorr => cmd::phase_corr::handle(port, payload),
            Cmd::SetOutputMask => cmd::output_mask::handle(port, payload),
            Cmd::SetPwe => cmd::pwe::handle(port, payload),
            Cmd::EmulateGpioIn => cmd::gpio_in::handle(port, payload),
            Cmd::SetGpioOut => self.gpio_out(port, payload),
            Cmd::ForceFan => cmd::force_fan::handle(port, payload),
            Cmd::Synchronize => self.sync(port),
            Cmd::SetMode => self.set_mode_cmd(payload),
            Cmd::Clear => self.clear(port),
        };
        match result {
            Ok(()) => 0,
            Err(err) => self.latch_error(err),
        }
    }

    pub(crate) fn set_and_wait_update<P: Port>(
        &self,
        port: &mut P,
        flag: u16,
    ) -> Result<(), Error> {
        fpga::set_and_wait_update(port, self.mode(), flag)
    }
}
