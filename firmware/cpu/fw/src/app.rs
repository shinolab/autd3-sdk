use core::cell::Cell;
use core::sync::atomic::{AtomicU8, AtomicU16, Ordering};

use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ReadTelemetryPayload;

use crate::cmd;
use crate::fifo::{FIFO_DEPTH, Fifo};
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

pub struct Cpu {
    mode: AtomicU8,
    last_seq: AtomicU8,
    last_cmd: AtomicU8,
    slots: [Cell<RxFrame>; FIFO_DEPTH as usize],
    fifo: Fifo,
    preempt_tx: AtomicU16,
    preempt_expected: AtomicU8,
    telemetry: [AtomicU8; Telemetry::CPU_COUNTER_COUNT],
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

macro_rules! cpu_new {
    () => {
        Self {
            mode: AtomicU8::new(Mode::Fifo as u8),
            last_seq: AtomicU8::new(0xFF),
            last_cmd: AtomicU8::new(0xFF),
            slots: [const { Cell::new(RxFrame::ZERO) }; FIFO_DEPTH as usize],
            fifo: Fifo::new(),
            preempt_tx: AtomicU16::new(0),
            preempt_expected: AtomicU8::new(0),
            telemetry: [const { AtomicU8::new(0) }; Telemetry::CPU_COUNTER_COUNT],
            al_err_ticks: Cell::new(0),
            proto: ProtoState::new(),
            silencer: cmd::silencer::SilencerGuard::new(),
            tx: AtomicU16::new(0),
        }
    };
}

#[cfg(not(loom))]
impl Cpu {
    #[must_use]
    pub const fn new() -> Self {
        cpu_new!()
    }
}

#[cfg(loom)]
impl Cpu {
    #[must_use]
    pub fn new() -> Self {
        cpu_new!()
    }
}

impl Cpu {
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
        self.fifo.reset();
        self.preempt_tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
        self.preempt_expected.store(0, Ordering::Relaxed);
    }

    pub(crate) fn reset_telemetry(&self) {
        for counter in &self.telemetry {
            counter.store(0, Ordering::Relaxed);
        }
        self.al_err_ticks.set(0);
    }

    fn bump(&self, id: Telemetry) {
        if let Some(counter) = self.telemetry.get(id as usize) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[must_use]
    pub fn telemetry(&self, id: Telemetry) -> u8 {
        self.telemetry
            .get(id as usize)
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
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
            cmd::failsafe::mute(port);
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

    #[cfg(all(test, not(loom)))]
    pub(crate) fn expected_seq(&self) -> u8 {
        self.proto.expected_seq.load(Ordering::Relaxed)
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn set_fw_version(&self, major: u8, minor: u8, patch: u8) {
        self.proto.fw_version_major.set(major);
        self.proto.fw_version_minor.set(minor);
        self.proto.fw_version_patch.set(patch);
    }

    #[cfg(all(test, not(loom)))]
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

        let head = self.fifo.head();
        let cmd = Cmd::from_u8(raw_cmd);
        let preempt = cmd == Some(Cmd::Reset);
        if preempt {
            self.preempt_tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
            self.preempt_expected.store(0, Ordering::Relaxed);
            self.fifo.request_flush(head);
        }

        let tail = self.fifo.tail_acquire();
        let inline_ok = preempt || (self.mode() == Mode::LowLatency && tail == head);
        if inline_ok {
            self.handle_frame(port, &RxFrame::from_wire(frame));
            self.last_seq.store(seq, Ordering::Relaxed);
            self.last_cmd.store(raw_cmd, Ordering::Relaxed);
            return;
        }

        if Fifo::is_full(head, tail) {
            self.bump(Telemetry::FifoDrop);
            return;
        }
        self.slots[Fifo::slot(head)].set(RxFrame::from_wire(frame));
        self.fifo.publish(head);
        self.last_seq.store(seq, Ordering::Relaxed);
        self.last_cmd.store(raw_cmd, Ordering::Relaxed);
    }

    pub fn process_one<P: Port>(&self, port: &mut P) -> bool {
        let flush_gen = self.fifo.begin_drain();
        let Some(tail) = self.fifo.next() else {
            return false;
        };
        let in_frame = self.slots[Fifo::slot(tail)].get();
        self.handle_frame(port, &in_frame);
        self.fifo.commit(tail);
        if self.fifo.flush_gen() != flush_gen {
            self.apply_preempt();
        }
        true
    }

    pub fn process_pending<P: Port>(&self, port: &mut P) {
        while self.process_one(port) {}
    }

    fn apply_preempt(&self) {
        self.proto.expected_seq.store(
            self.preempt_expected.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.tx
            .store(self.preempt_tx.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    fn handle_frame<P: Port>(&self, port: &mut P, in_frame: &RxFrame) {
        let cmd = Cmd::from_u8(in_frame.cmd);
        if cmd == Some(Cmd::Reset) {
            self.apply_preempt();
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

    fn read_telemetry<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let Ok((p, _)) = ReadTelemetryPayload::ref_from_prefix(payload) else {
            return self.latch_error(Error::InvalidPayload);
        };
        match Telemetry::from_u8(p.counter_id) {
            Some(Telemetry::SyncResync) => {
                (fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_FPGA_STATE) >> 8) as u8
            }
            Some(id) => self.telemetry(id),
            None => self.latch_error(Error::InvalidPayload),
        }
    }

    fn dispatch<P: Port>(&self, port: &mut P, cmd: Cmd, payload: &[u8]) -> u8 {
        let result = match cmd {
            Cmd::Reset | Cmd::Nop => Ok(()),
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
            Cmd::ReadTelemetry => return self.read_telemetry(port, payload),
            Cmd::ReadFpgaFunctions => {
                return (fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) >> 8)
                    as u8;
            }
            Cmd::WritePatternBuffer => cmd::write_pattern::handle(port, payload),
            Cmd::WritePatternCompressed => cmd::write_pattern_compressed::handle(port, payload),
            Cmd::WritePatternFused => self.write_pattern_fused(port, payload),
            Cmd::WriteModulationBuffer => cmd::write_mod::handle(port, payload),
            Cmd::WriteModulationFused => self.write_mod_fused(port, payload),
            Cmd::ConfigModulation => self.config_mod(port, payload),
            Cmd::ConfigPattern => self.config_pattern(port, payload),
            Cmd::ChangeModulationBank => self.change_mod_bank(port, payload),
            Cmd::ChangePatternBank => self.change_pattern_bank(port, payload),
            Cmd::SetSilencer => self.set_silencer(port, payload),
            Cmd::SetPhaseCorrection => cmd::phase_corr::handle(port, payload),
            Cmd::SetOutputMask => cmd::output_mask::handle(port, payload),
            Cmd::SetPulseWidthTable => cmd::pwe::handle(port, payload),
            Cmd::EmulateGpioIn => cmd::gpio_in::handle(port, payload),
            Cmd::SetGpioOut => self.gpio_out(port, payload),
            Cmd::ForceFan => cmd::force_fan::handle(port, payload),
            Cmd::Synchronize => self.sync(port),
            Cmd::SetMode => self.set_mode_cmd(payload),
            Cmd::Clear => self.clear(port),
            _ => Err(Error::UnknownCmd),
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
