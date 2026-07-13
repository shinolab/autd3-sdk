use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};

use crate::cmd;
use crate::fpga;
use crate::params::{
    ADDR_FPGA_STATE, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH,
    BRAM_SELECT_CONTROLLER,
};
use crate::port::Port;
use crate::proto::{
    AL_STATUS_CODE_SM_WATCHDOG, AL_STATUS_CODE_SYNC_ERROR, CMD_CHANGE_MOD_BANK,
    CMD_CHANGE_PATTERN_BANK, CMD_CLEAR, CMD_CONFIG_MOD, CMD_CONFIG_PATTERN, CMD_EMULATE_GPIO_IN,
    CMD_FORCE_FAN, CMD_NOP, CMD_READ_CPU_FW_VERSION_MAJOR, CMD_READ_CPU_FW_VERSION_MINOR,
    CMD_READ_CPU_FW_VERSION_PATCH, CMD_READ_ERROR_DETAIL, CMD_READ_FPGA_FUNCTIONS,
    CMD_READ_FPGA_FW_VERSION_MAJOR, CMD_READ_FPGA_FW_VERSION_MINOR, CMD_READ_FPGA_FW_VERSION_PATCH,
    CMD_READ_FPGA_STATE, CMD_READ_TELEMETRY, CMD_RESET, CMD_SET_GPIO_OUT, CMD_SET_MODE,
    CMD_SET_OUTPUT_MASK, CMD_SET_PHASE_CORR, CMD_SET_PWE, CMD_SET_SILENCER, CMD_STOP,
    CMD_SYNCHRONIZE, CMD_WRITE_MOD_BUFFER, CMD_WRITE_PATTERN_BUFFER, CMD_WRITE_PATTERN_COMPRESSED,
    CMD_XOR_HASH, ERR_INVALID_PAYLOAD, ERR_NONE, ERR_UNKNOWN_CMD, FAILSAFE_TICKS, MODE_FIFO,
    MODE_LOW_LATENCY, ProtoState, READ_TELEMETRY_OFFSET_COUNTER_ID, RxFrame, TELEMETRY_COUNT,
    TELEMETRY_DEDUP, TELEMETRY_DISPATCH_ERROR, TELEMETRY_FAILSAFE, TELEMETRY_FIFO_DROP,
    TELEMETRY_PROCESSED, TELEMETRY_SEQ_MISMATCH, TxFrame, WIRE_RX_FRAME_BYTES,
};

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
    telemetry: [AtomicU8; TELEMETRY_COUNT],
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
            mode: AtomicU8::new(MODE_FIFO),
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
            telemetry: [const { AtomicU8::new(0) }; TELEMETRY_COUNT],
            al_err_ticks: Cell::new(0),
            proto: ProtoState::new(),
            silencer: cmd::silencer::SilencerGuard::new(),
            tx: AtomicU16::new(0),
        }
    }

    pub fn init<P: Port>(&self, port: &mut P) {
        self.set_mode(MODE_FIFO);
        self.proto.init();
        let fpga_err = fpga::init(port, self.mode());
        if fpga_err != ERR_NONE {
            self.proto.error_detail.set(fpga_err);
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

    fn bump(&self, id: u8) {
        self.telemetry[id as usize].fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn telemetry(&self, id: u8) -> u8 {
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
            self.bump(TELEMETRY_FAILSAFE);
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
    pub(crate) fn set_error_detail(&self, code: u8) {
        self.proto.error_detail.set(code);
    }

    #[must_use]
    pub fn mode(&self) -> u8 {
        self.mode.load(Ordering::Relaxed)
    }

    pub(crate) fn set_mode(&self, mode: u8) {
        self.mode.store(mode, Ordering::Relaxed);
    }

    pub fn recv_ethercat<P: Port>(&self, port: &mut P, frame: &[u8; WIRE_RX_FRAME_BYTES]) {
        let seq = frame[0];
        let cmd = frame[1];
        if seq == self.last_seq.load(Ordering::Relaxed)
            && cmd == self.last_cmd.load(Ordering::Relaxed)
        {
            self.bump(TELEMETRY_DEDUP);
            return;
        }

        let head = self.fifo_head.load(Ordering::Relaxed);
        let preempt = cmd == CMD_RESET || cmd == CMD_STOP;
        if preempt {
            if cmd == CMD_RESET {
                self.preempt_tx.store(pack_tx(0xFF, 0), Ordering::Relaxed);
                self.preempt_expected.store(0, Ordering::Relaxed);
                self.preempt_mute.store(false, Ordering::Relaxed);
            } else {
                self.preempt_tx
                    .store(pack_tx(seq, ERR_NONE), Ordering::Relaxed);
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
        let inline_ok = preempt || (self.mode() == MODE_LOW_LATENCY && tail == head);
        if inline_ok {
            self.handle_frame(port, &RxFrame::from_wire(frame));
            self.last_seq.store(seq, Ordering::Relaxed);
            self.last_cmd.store(cmd, Ordering::Relaxed);
            return;
        }

        if head.wrapping_sub(tail) >= FIFO_CAPACITY {
            self.bump(TELEMETRY_FIFO_DROP);
            return;
        }
        self.fifo[(head & FIFO_MASK) as usize].set(RxFrame::from_wire(frame));
        self.fifo_head
            .store(head.wrapping_add(1), Ordering::Release);
        self.last_seq.store(seq, Ordering::Relaxed);
        self.last_cmd.store(cmd, Ordering::Relaxed);
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
        if in_frame.cmd == CMD_RESET || in_frame.cmd == CMD_STOP {
            self.apply_preempt(port);
            return;
        }
        if in_frame.seq == self.proto.expected_seq.load(Ordering::Relaxed) {
            self.proto
                .expected_seq
                .store(in_frame.seq.wrapping_add(1), Ordering::Relaxed);
            let data = self.dispatch(port, in_frame);
            self.tx
                .store(pack_tx(in_frame.seq, data), Ordering::Relaxed);
            self.bump(TELEMETRY_PROCESSED);
        } else {
            self.bump(TELEMETRY_SEQ_MISMATCH);
        }
    }

    fn latch_error(&self, data: u8) -> u8 {
        if data != ERR_NONE {
            self.proto.error_detail.set(data);
            self.bump(TELEMETRY_DISPATCH_ERROR);
        }
        data
    }

    fn read_telemetry(&self, payload: &[u8]) -> u8 {
        let id = payload[READ_TELEMETRY_OFFSET_COUNTER_ID];
        if usize::from(id) >= TELEMETRY_COUNT {
            return self.latch_error(ERR_INVALID_PAYLOAD);
        }
        self.telemetry(id)
    }

    fn dispatch<P: Port>(&self, port: &mut P, in_frame: &RxFrame) -> u8 {
        let payload = &in_frame.payload;
        let data = match in_frame.cmd {
            CMD_XOR_HASH => cmd::xor_hash::handle(port, payload),
            CMD_READ_CPU_FW_VERSION_MAJOR => return self.proto.fw_version_major.get(),
            CMD_READ_CPU_FW_VERSION_MINOR => return self.proto.fw_version_minor.get(),
            CMD_READ_CPU_FW_VERSION_PATCH => return self.proto.fw_version_patch.get(),
            CMD_READ_FPGA_FW_VERSION_MAJOR => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) as u8;
            }
            CMD_READ_FPGA_FW_VERSION_MINOR => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MINOR) as u8;
            }
            CMD_READ_FPGA_FW_VERSION_PATCH => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_PATCH) as u8;
            }
            CMD_READ_ERROR_DETAIL => return self.proto.error_detail.get(),
            CMD_READ_FPGA_STATE => {
                return fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_FPGA_STATE) as u8;
            }
            CMD_READ_TELEMETRY => return self.read_telemetry(payload),
            CMD_READ_FPGA_FUNCTIONS => {
                return (fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) >> 8)
                    as u8;
            }
            CMD_WRITE_PATTERN_BUFFER => cmd::write_pattern::handle(port, payload),
            CMD_WRITE_PATTERN_COMPRESSED => cmd::write_pattern_compressed::handle(port, payload),
            CMD_WRITE_MOD_BUFFER => cmd::write_mod::handle(port, payload),
            CMD_CONFIG_MOD => self.config_mod(port, payload),
            CMD_CONFIG_PATTERN => self.config_pattern(port, payload),
            CMD_CHANGE_MOD_BANK => self.change_mod_bank(port, payload),
            CMD_CHANGE_PATTERN_BANK => self.change_pattern_bank(port, payload),
            CMD_SET_SILENCER => self.set_silencer(port, payload),
            CMD_SET_PHASE_CORR => cmd::phase_corr::handle(port, payload),
            CMD_SET_OUTPUT_MASK => cmd::output_mask::handle(port, payload),
            CMD_SET_PWE => cmd::pwe::handle(port, payload),
            CMD_EMULATE_GPIO_IN => cmd::gpio_in::handle(port, payload),
            CMD_SET_GPIO_OUT => self.gpio_out(port, payload),
            CMD_FORCE_FAN => cmd::force_fan::handle(port, payload),
            CMD_SYNCHRONIZE => self.sync(port),
            CMD_SET_MODE => self.set_mode_cmd(payload),
            CMD_CLEAR => self.clear(port),
            CMD_NOP => return ERR_NONE,
            _ => ERR_UNKNOWN_CMD,
        };
        self.latch_error(data)
    }

    pub(crate) fn set_and_wait_update<P: Port>(&self, port: &mut P, flag: u16) -> u8 {
        fpga::set_and_wait_update(port, self.mode(), flag)
    }
}
