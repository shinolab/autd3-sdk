use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use zerocopy::IntoBytes;

use autd3_cpu_wire::payload::{FirmwareInfo, expect_empty};

use crate::cmd;
use crate::fifo::{FIFO_DEPTH, Fifo};
use crate::fpga;
use crate::params::{
    ADDR_FPGA_STATE, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH,
    BRAM_SELECT_CONTROLLER,
};
use crate::port::Port;
use crate::proto::{
    Cmd, Disposition, Drained, Error, FAILSAFE_TIMEOUT_MS, FRAME_BYTES_MAX, Mode,
    REPLY_DATA_BYTES_MAX, Reply, RxFrame, Telemetry,
};
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

pub struct Cpu {
    mode: AtomicU8,
    last_seq: AtomicU8,
    last_cmd: AtomicU8,
    slots: [Cell<RxFrame>; FIFO_DEPTH as usize],
    fifo: Fifo,
    telemetry: [AtomicU32; Telemetry::CPU_COUNTER_COUNT],
    failsafe_fired: AtomicBool,
    expected_seq: AtomicU8,
    pub(crate) silencer: cmd::silencer::SilencerGuard,
    pub(crate) update: cmd::update::UpdateSession,
    pub(crate) fpga_update: cmd::fpga_update::FpgaUpdateSession,
    reply_meta: AtomicU32,
    reply_data: [[AtomicU32; REPLY_WORDS]; 2],
}

const REPLY_WORDS: usize = REPLY_DATA_BYTES_MAX / 4;

const _: () = assert!(REPLY_DATA_BYTES_MAX.is_multiple_of(4));
const _: () = assert!(FIFO_DEPTH as usize > autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES);

fn pack_meta(ack: u8, status: u8, len: u8, bank: usize) -> u32 {
    u32::from(ack) | (u32::from(status) << 8) | (u32::from(len) << 16) | ((bank as u32) << 24)
}

type Outcome = Result<ReplyData, Error>;

pub(crate) struct ReplyData {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

impl ReplyData {
    pub(crate) const EMPTY: Self = Self {
        len: 0,
        bytes: [0; REPLY_DATA_BYTES_MAX],
    };

    pub(crate) fn from_slice(data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut bytes = [0; REPLY_DATA_BYTES_MAX];
        bytes[..len].copy_from_slice(&data[..len]);
        Self {
            len: len as u8,
            bytes,
        }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

const fn takes_payload(cmd: Cmd) -> bool {
    !matches!(
        cmd,
        Cmd::Reset
            | Cmd::Nop
            | Cmd::ReadFpgaState
            | Cmd::ReadTelemetry
            | Cmd::ReadFirmwareInfo
            | Cmd::UpdateCommit
            | Cmd::UpdateActivate
            | Cmd::UpdateConfirm
            | Cmd::FpgaUpdateCommit
            | Cmd::FpgaUpdateActivate
            | Cmd::Synchronize
            | Cmd::Clear
    )
}

fn empty(result: Result<(), Error>) -> Outcome {
    result.map(|()| ReplyData::EMPTY)
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
            mode: AtomicU8::new(Mode::Fifo as u8),
            last_seq: AtomicU8::new(0xFF),
            last_cmd: AtomicU8::new(0xFF),
            slots: [const { Cell::new(RxFrame::ZERO) }; FIFO_DEPTH as usize],
            fifo: Fifo::new(),
            telemetry: [const { AtomicU32::new(0) }; Telemetry::CPU_COUNTER_COUNT],
            failsafe_fired: AtomicBool::new(false),
            expected_seq: AtomicU8::new(0),
            silencer: cmd::silencer::SilencerGuard::new(),
            update: cmd::update::UpdateSession::new(),
            fpga_update: cmd::fpga_update::FpgaUpdateSession::new(),
            reply_meta: AtomicU32::new(0),
            reply_data: [const { [const { AtomicU32::new(0) }; REPLY_WORDS] }; 2],
        }
    }
}

impl Cpu {
    pub fn init<P: Port>(&self, port: &mut P) {
        self.set_mode(Mode::Fifo);
        self.expected_seq.store(0, Ordering::Relaxed);
        let _ = fpga::init(port, self.mode());
        self.silencer.init();
        self.update.init();
        self.fpga_update.init();
        self.reset_telemetry();
        self.set_reply(0xFF, 0, &[]);
        self.last_seq.store(0xFF, Ordering::Relaxed);
        self.last_cmd.store(0xFF, Ordering::Relaxed);
        self.fifo.reset();
    }

    pub(crate) fn reinit_fpga<P: Port>(&self, port: &mut P) {
        let _ = fpga::init(port, self.mode());
        self.silencer.init();
    }

    pub fn mark_boot_attempt<P: Port>(&self, port: &mut P) {
        let _ = self.record_boot_attempt(port);
    }

    #[must_use]
    pub fn booted_slot(&self) -> Option<autd3_cpu_wire::update::Slot> {
        self.update.boot_slot()
    }

    pub(crate) fn reset_telemetry(&self) {
        for counter in &self.telemetry {
            counter.store(0, Ordering::Relaxed);
        }
        self.failsafe_fired.store(false, Ordering::Relaxed);
    }

    fn bump(&self, id: Telemetry) {
        if let Some(counter) = self.telemetry.get(id as usize) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[must_use]
    pub fn telemetry(&self, id: Telemetry) -> u32 {
        self.telemetry
            .get(id as usize)
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }

    pub fn tick_1ms<P: Port>(&self, port: &mut P) {
        self.update_tick(port);
        self.fpga_update_tick(port);
        let silent = port
            .host_idle_ms()
            .is_some_and(|ms| ms >= FAILSAFE_TIMEOUT_MS);
        if !silent {
            self.failsafe_fired.store(false, Ordering::Relaxed);
            return;
        }
        if !self.failsafe_fired.swap(true, Ordering::Relaxed) {
            cmd::failsafe::mute(port);
            self.bump(Telemetry::Failsafe);
        }
    }

    #[must_use]
    pub fn reply(&self) -> Reply {
        let meta = self.reply_meta.load(Ordering::Acquire);
        let len = usize::from((meta >> 16) as u8).min(REPLY_DATA_BYTES_MAX);
        let bank = (meta >> 24) as usize & 1;
        let mut data = [0u8; REPLY_DATA_BYTES_MAX];
        for (chunk, word) in data.chunks_mut(4).zip(&self.reply_data[bank]) {
            chunk.copy_from_slice(&word.load(Ordering::Relaxed).to_le_bytes());
        }
        Reply::new(meta as u8, (meta >> 8) as u8, &data[..len])
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn expected_seq(&self) -> u8 {
        self.expected_seq.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        Mode::from_u8(self.mode.load(Ordering::Relaxed)).unwrap_or(Mode::Fifo)
    }

    pub(crate) fn set_mode(&self, mode: Mode) {
        self.mode.store(mode as u8, Ordering::Relaxed);
    }

    pub fn recv_frame<P: Port>(&self, port: &mut P, frame: &[u8], msg_id: u16) -> Disposition {
        let (Some(&seq), Some(&raw_cmd)) = (frame.first(), frame.get(1)) else {
            return Disposition::Dropped;
        };
        if frame.len() > FRAME_BYTES_MAX {
            return Disposition::Dropped;
        }
        if seq == self.last_seq.load(Ordering::Relaxed)
            && raw_cmd == self.last_cmd.load(Ordering::Relaxed)
        {
            self.bump(Telemetry::Dedup);
            return Disposition::Reply;
        }

        let head = self.fifo.head();
        let cmd = Cmd::from_u8(raw_cmd);
        let preempt = cmd == Some(Cmd::Reset);
        if preempt {
            self.fifo.request_flush(head);
        }

        let deferred = cmd.is_some_and(|cmd| {
            cmd::update::is_update_cmd(cmd) || cmd::fpga_update::is_fpga_update_cmd(cmd)
        });
        let tail = self.fifo.tail_acquire();
        let inline_ok = preempt || (self.mode() == Mode::LowLatency && tail == head && !deferred);
        if inline_ok {
            self.handle_frame(port, &RxFrame::from_frame(frame, msg_id));
            self.last_seq.store(seq, Ordering::Relaxed);
            self.last_cmd.store(raw_cmd, Ordering::Relaxed);
            return Disposition::Reply;
        }

        if Fifo::is_full(head, tail) {
            self.bump(Telemetry::FifoDrop);
            return Disposition::Dropped;
        }
        self.slots[Fifo::slot(head)].set(RxFrame::from_frame(frame, msg_id));
        self.fifo.publish(head);
        self.last_seq.store(seq, Ordering::Relaxed);
        self.last_cmd.store(raw_cmd, Ordering::Relaxed);
        Disposition::Deferred
    }

    pub fn process_one<P: Port>(&self, port: &mut P) -> Drained {
        let flush_gen = self.fifo.begin_drain();
        self.drain_step(port, flush_gen)
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn begin_drain(&self) -> u16 {
        self.fifo.begin_drain()
    }

    pub(crate) fn drain_step<P: Port>(&self, port: &mut P, flush_gen: u16) -> Drained {
        let Some(tail) = self.fifo.next() else {
            return Drained::Empty;
        };
        let in_frame = self.slots[Fifo::slot(tail)].get();
        self.handle_frame(port, &in_frame);
        let flushed = self.fifo.is_before_flush(flush_gen, tail);
        if flushed {
            self.apply_preempt();
        }
        self.fifo.commit(tail);
        if flushed {
            Drained::Flushed
        } else {
            Drained::Completed {
                msg_id: in_frame.msg_id,
            }
        }
    }

    pub fn process_pending<P: Port>(&self, port: &mut P) {
        while self.process_one(port) != Drained::Empty {}
    }

    fn apply_preempt(&self) {
        self.expected_seq.store(0, Ordering::Relaxed);
        self.set_reply(0xFF, 0, &[]);
    }

    fn set_reply(&self, ack: u8, status: u8, data: &[u8]) {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let current = self.reply_meta.load(Ordering::Relaxed);
        let bank = ((current >> 24) as usize & 1) ^ 1;
        let mut padded = [0u8; REPLY_DATA_BYTES_MAX];
        padded[..len].copy_from_slice(&data[..len]);
        for (chunk, word) in padded.chunks(4).zip(&self.reply_data[bank]) {
            word.store(
                u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                Ordering::Relaxed,
            );
        }
        let _ = self.reply_meta.compare_exchange(
            current,
            pack_meta(ack, status, len as u8, bank),
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    fn handle_frame<P: Port>(&self, port: &mut P, in_frame: &RxFrame) {
        let cmd = Cmd::from_u8(in_frame.cmd);
        if cmd == Some(Cmd::Reset) {
            self.apply_preempt();
            return;
        }
        if in_frame.seq == self.expected_seq.load(Ordering::Relaxed) {
            self.expected_seq
                .store(in_frame.seq.wrapping_add(1), Ordering::Relaxed);
            let outcome = match cmd {
                Some(cmd) => self.dispatch(port, cmd, in_frame.payload()),
                None => Err(Error::UnknownCmd),
            };
            match outcome {
                Ok(data) => self.set_reply(in_frame.seq, 0, data.as_slice()),
                Err(err) => {
                    self.bump(Telemetry::DispatchError);
                    self.set_reply(in_frame.seq, err as u8, &[]);
                }
            }
            self.bump(Telemetry::Processed);
        } else {
            self.bump(Telemetry::SeqMismatch);
        }
    }

    fn read_telemetry<P: Port>(&self, port: &mut P) -> ReplyData {
        let mut bytes = [0u8; Telemetry::REPLY_BYTES];
        for (chunk, id) in bytes
            .chunks_mut(Telemetry::COUNTER_BYTES)
            .zip(Telemetry::ALL)
        {
            let value = match id {
                Telemetry::SyncResync => {
                    u32::from(fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_FPGA_STATE) >> 8)
                }
                id => self.telemetry(*id),
            };
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        ReplyData::from_slice(&bytes)
    }

    fn read_firmware_info<P: Port>(&self, port: &mut P) -> ReplyData {
        let major = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR);
        let info = FirmwareInfo {
            cpu_version: [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH],
            fpga_version: [
                major as u8,
                fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MINOR) as u8,
                fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_PATCH) as u8,
            ],
            fpga_functions: (major >> 8) as u8,
            fpga_boot_image: self.fpga_boot_image(port) as u8,
        };
        ReplyData::from_slice(info.as_bytes())
    }

    fn dispatch<P: Port>(&self, port: &mut P, cmd: Cmd, payload: &[u8]) -> Outcome {
        if self.fpga_update.is_locked() && !cmd::fpga_update::allowed_while_locked(cmd) {
            return Err(Error::FpgaUpdateInProgress);
        }
        if !takes_payload(cmd) {
            expect_empty(payload)?;
        }
        let result = match cmd {
            Cmd::Reset | Cmd::Nop => Ok(()),
            Cmd::ReadFpgaState => {
                return Ok(ReplyData::from_slice(&[fpga::read(
                    port,
                    BRAM_SELECT_CONTROLLER,
                    ADDR_FPGA_STATE,
                ) as u8]));
            }
            Cmd::ReadTelemetry => return Ok(self.read_telemetry(port)),
            Cmd::ReadFirmwareInfo => return Ok(self.read_firmware_info(port)),
            Cmd::WriteFociBuffer => cmd::write_foci::handle(port, payload),
            Cmd::WritePatternRaw => cmd::write_pattern_raw::handle(port, payload),
            Cmd::WritePatternPhase => cmd::write_pattern_phase::handle(port, payload),
            Cmd::WriteModulationBuffer => cmd::write_mod::handle(port, payload),
            Cmd::ConfigModulation => self.config_mod(port, payload),
            Cmd::ConfigPattern => self.config_pattern(port, payload),
            Cmd::ActivateModulationBank => self.activate_mod_bank(port, payload),
            Cmd::ActivatePatternBank => self.activate_pattern_bank(port, payload),
            Cmd::SetSilencer => self.set_silencer(port, payload),
            Cmd::SetPhaseCorrection => cmd::phase_corr::handle(port, payload),
            Cmd::SetOutputMask => cmd::output_mask::handle(port, payload),
            Cmd::SetPulseWidthTable => cmd::pwe::handle(port, payload),
            Cmd::EmulateGpioIn => cmd::gpio_in::handle(port, payload),
            Cmd::SetGpioOut => self.gpio_out(port, payload),
            Cmd::ForceFan => cmd::force_fan::handle(port, payload),
            Cmd::UpdateBegin => self.update_begin(port, payload),
            Cmd::UpdateChunk => self.update_chunk(port, payload),
            Cmd::UpdateCommit => self.update_commit(port),
            Cmd::UpdateActivate => self.update_activate(),
            Cmd::UpdateConfirm => self.update_confirm(port),
            Cmd::FpgaUpdateBegin => self.fpga_update_begin(port, payload),
            Cmd::FpgaUpdateChunk => self.fpga_update_chunk(port, payload),
            Cmd::FpgaUpdateCommit => self.fpga_update_commit(port),
            Cmd::FpgaUpdateActivate => self.fpga_update_activate(),
            Cmd::Synchronize => self.sync(port),
            Cmd::SetMode => self.set_mode_cmd(payload),
            Cmd::Clear => self.clear(port),
            _ => Err(Error::UnknownCmd),
        };
        empty(result)
    }

    pub(crate) fn set_and_wait_update<P: Port>(
        &self,
        port: &mut P,
        flag: u16,
    ) -> Result<(), Error> {
        fpga::set_and_wait_update(port, self.mode(), flag)
    }
}
