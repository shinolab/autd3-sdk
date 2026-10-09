use core::num::NonZeroU64;
use std::boxed::Box;
use std::collections::VecDeque;
use std::rc::Rc;
use std::vec;
use std::vec::Vec;

use zerocopy::{Immutable, IntoBytes};

use crate::app::Cpu;
use crate::fpga::PWE_TABLE_SIZE;
use crate::fpga_params::{
    ADDR_CTL_FLAG, ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1, ADDR_FLASH_CMD, ADDR_FLASH_LEN_0,
    ADDR_FLASH_LEN_1, ADDR_FLASH_RESULT_0, ADDR_FLASH_RESULT_1, ADDR_FLASH_STATUS,
    ADDR_FLASH_USR_ACCESS_0, ADDR_FLASH_USR_ACCESS_1, ADDR_FUNCTION_BITS, ADDR_MOD_FREQ_DIV0,
    ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_MOD_TRANSITION_MODE, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MEM_WR_BANK,
    ADDR_PATTERN_MEM_WR_PAGE, ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK,
    ADDR_PATTERN_TRANSITION_MODE, ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG,
    ADDR_SILENCER_SET_RESULT, BramSelect, CtlFlags, FLASH_BUF_BYTES, FlashErr, FlashOp,
    FunctionBits, NUM_BANKS,
};
use crate::port::{FlashError, Port};
use crate::proto::{
    Cmd, Drained, EMISSION_RAM_WORDS, Error, FrameHeader, MOD_BUFFER_SAMPLES, OUTPUT_MASK_WORDS,
    PAYLOAD_BYTES, Reply, RxFrame, Telemetry,
};
use autd3_cpu_wire::fpga_update::{FPGA_FLASH_BYTES, FPGA_GOLDEN_REGION_END, FPGA_SECTOR_BYTES};
use autd3_cpu_wire::update::{FLASH_BYTES, FLASH_SECTOR_BYTES, LOADER_REGION_END, crc32};

pub(crate) const MOD_RAM_WORDS: usize = (MOD_BUFFER_SAMPLES / 2) as usize;
pub(crate) const EM_RAM_WORDS: usize = EMISSION_RAM_WORDS as usize;

const LATCH_MASK: CtlFlags = CtlFlags::MOD_SET
    .union(CtlFlags::PATTERN_SET)
    .union(CtlFlags::SILENCER_SET)
    .union(CtlFlags::DEBUG_SET)
    .union(CtlFlags::SYNC_SET);

const MOD_LATCHED_REGS: [(u16, usize); 4] = [
    (ADDR_MOD_REQ_RD_BANK, 1),
    (ADDR_MOD_TRANSITION_MODE, 1),
    (ADDR_MOD_FREQ_DIV0, NUM_BANKS),
    (ADDR_MOD_REP0, NUM_BANKS),
];
const PATTERN_LATCHED_REGS: [(u16, usize); 4] = [
    (ADDR_PATTERN_REQ_RD_BANK, 1),
    (ADDR_PATTERN_TRANSITION_MODE, 1),
    (ADDR_PATTERN_FREQ_DIV0, NUM_BANKS),
    (ADDR_PATTERN_REP0, NUM_BANKS),
];

pub(crate) struct MockPort {
    pub ctl: Box<[u16; 256]>,
    pub latched: Box<[u16; 256]>,
    pub reject_latch: CtlFlags,
    pub latch_log: Vec<CtlFlags>,
    pub ctl_flag_writes: Vec<CtlFlags>,
    pub phase_corr: Box<[u16; 256]>,
    pub output_mask: Box<[u16; OUTPUT_MASK_WORDS]>,
    pub pwe: Box<[u16; PWE_TABLE_SIZE]>,
    pub mod_ram: Vec<Vec<u16>>,
    pub em_ram: Vec<Vec<u16>>,
    pub latch_count: [u32; 16],
    pub next_sync_edge: Option<NonZeroU64>,
    pub sync_guard_ns: Option<u32>,
    pub ptp_config: Option<autd3_cpu_wire::config::PtpConfig>,
    pub fpga_bus_wait: Option<autd3_cpu_wire::config::FpgaBusWait>,
    pub ctl_flag_reads: u32,
    pub fpga_flash_status_reads: u32,
    pub sys_time: Option<u64>,
    pub sys_time_reads: VecDeque<Option<u64>>,
    pub next_sync_edges: VecDeque<Option<NonZeroU64>>,
    pub host_idle_ms: Option<u32>,
    pub ptp_unlocked_ms: Option<u32>,
    pub latch_stuck: bool,
    pub flash: Vec<u8>,
    pub flash_fail: bool,
    pub flash_write_fail_after: Option<usize>,
    pub flash_write_silent: bool,
    pub erased: Vec<(u32, u32)>,
    pub reset_count: u32,
    pub fpga_flash: Vec<u8>,
    pub fpga_flash_reg: [u16; 16],
    pub fpga_flash_buf: Box<[u16; FLASH_BUF_BYTES / 2]>,
    pub fpga_flash_ops: Vec<(u8, u32, u32)>,
    pub fpga_flash_err: Option<FlashErr>,
    pub fpga_flash_hang: bool,
    pub fpga_flash_dropped_reg: Option<u16>,
    pub fpga_flash_target_unflushed: bool,
    pub fpga_flash_cmds_before_flush: u32,
    pub fpga_usr_access: u32,
    pub fpga_reboots: u32,
    pub fpga_reboots_to_ignore: u32,
    isr_frame: Option<(Rc<Cpu>, u8, u8)>,
    pub isr_seen_reply: Option<Reply>,
}

impl MockPort {
    pub(crate) fn new() -> Self {
        let mut ctl = Box::new([0; 256]);
        ctl[ADDR_FUNCTION_BITS as usize] = u16::from(FunctionBits::STRICT_SILENCER_GUARD.bits());
        Self {
            ctl,
            latched: Box::new([0; 256]),
            reject_latch: CtlFlags::empty(),
            latch_log: Vec::new(),
            ctl_flag_writes: Vec::new(),
            phase_corr: Box::new([0; 256]),
            output_mask: Box::new([0; OUTPUT_MASK_WORDS]),
            pwe: Box::new([0; PWE_TABLE_SIZE]),
            mod_ram: vec![vec![0; MOD_RAM_WORDS]; NUM_BANKS],
            em_ram: vec![vec![0; EM_RAM_WORDS]; NUM_BANKS],
            latch_count: [0; 16],
            next_sync_edge: None,
            sync_guard_ns: None,
            ptp_config: None,
            fpga_bus_wait: None,
            ctl_flag_reads: 0,
            fpga_flash_status_reads: 0,
            sys_time: Some(0),
            sys_time_reads: VecDeque::new(),
            next_sync_edges: VecDeque::new(),
            host_idle_ms: None,
            ptp_unlocked_ms: None,
            latch_stuck: false,
            flash: vec![0xFF; FLASH_BYTES as usize],
            flash_fail: false,
            flash_write_fail_after: None,
            flash_write_silent: false,
            erased: Vec::new(),
            reset_count: 0,
            fpga_flash: Vec::new(),
            fpga_flash_reg: [0; 16],
            fpga_flash_buf: Box::new([0; FLASH_BUF_BYTES / 2]),
            fpga_flash_ops: Vec::new(),
            fpga_flash_err: None,
            fpga_flash_hang: false,
            fpga_flash_dropped_reg: None,
            fpga_flash_target_unflushed: false,
            fpga_flash_cmds_before_flush: 0,
            fpga_usr_access: 0,
            fpga_reboots: 0,
            fpga_reboots_to_ignore: 0,
            isr_frame: None,
            isr_seen_reply: None,
        }
    }

    pub(crate) fn latch_count_of(&self, flag: CtlFlags) -> u32 {
        (0..16usize)
            .find(|bit| (flag.bits() & (1 << bit)) != 0)
            .map_or(0, |bit| self.latch_count[bit])
    }

    fn write_controller(&mut self, addr: u16, value: u16) {
        let select = (addr >> 8) as u8;
        match BramSelect::from_u8(select) {
            Some(BramSelect::Controller) => {
                if addr == ADDR_CTL_FLAG {
                    self.ctl_flag_writes.push(CtlFlags::from_bits_retain(value));
                    let latching = CtlFlags::from_bits_retain(value) & LATCH_MASK;
                    for bit in 0..16usize {
                        if (latching.bits() & (1 << bit)) != 0 {
                            self.latch_count[bit] += 1;
                        }
                    }
                    self.ctl[ADDR_CTL_FLAG as usize] = if self.latch_stuck {
                        value
                    } else {
                        self.latch(latching);
                        value & !LATCH_MASK.bits()
                    };
                } else {
                    self.ctl[(addr & 0xFF) as usize] = value;
                }
            }
            Some(BramSelect::Flash) if self.fpga_flash_dropped_reg == Some(addr & 0xFF) => {}
            Some(BramSelect::Flash) => {
                self.fpga_flash_reg[(addr & 0xF) as usize] = value;
                if addr & 0xFF != ADDR_FLASH_CMD {
                    self.fpga_flash_target_unflushed = true;
                } else if self.fpga_flash_target_unflushed {
                    self.fpga_flash_cmds_before_flush += 1;
                }
                if addr & 0xFF == ADDR_FLASH_CMD {
                    self.run_flash_command(value as u8);
                }
            }
            _ if select >> 1 == BramSelect::FlashBuf.as_u8() >> 1 => {
                self.fpga_flash_buf[(addr & 0x1FF) as usize] = value;
            }
            Some(BramSelect::PhaseCorr) => self.phase_corr[(addr & 0xFF) as usize] = value,
            Some(BramSelect::OutputMask) => {
                self.output_mask[(addr as usize) & (OUTPUT_MASK_WORDS - 1)] = value;
            }
            Some(BramSelect::PweTable) => self.pwe[(addr as usize) & (PWE_TABLE_SIZE - 1)] = value,
            _ => {}
        }
    }

    fn latch(&mut self, flags: CtlFlags) {
        if flags.is_empty() {
            return;
        }
        self.latch_log.push(flags);
        let rejected = flags & self.reject_latch;
        let kept = CtlFlags::from_bits_retain(self.ctl[ADDR_SILENCER_SET_RESULT as usize]) & !flags;
        self.ctl[ADDR_SILENCER_SET_RESULT as usize] = (kept | rejected).bits();
        let accepted = flags & !rejected;
        if accepted.contains(CtlFlags::MOD_SET) {
            self.copy_latched(&MOD_LATCHED_REGS);
        }
        if accepted.contains(CtlFlags::PATTERN_SET) {
            self.copy_latched(&PATTERN_LATCHED_REGS);
        }
        if accepted.contains(CtlFlags::SILENCER_SET) {
            let regs = ADDR_SILENCER_FLAG as usize..=ADDR_SILENCER_COMPLETION_STEPS_PHASE as usize;
            self.latched[regs.clone()].copy_from_slice(&self.ctl[regs]);
        }
    }

    fn copy_latched(&mut self, regs: &[(u16, usize)]) {
        for &(base, words) in regs {
            let range = base as usize..base as usize + words;
            self.latched[range.clone()].copy_from_slice(&self.ctl[range]);
        }
    }

    pub(crate) fn fpga_flash_mut(&mut self) -> &mut Vec<u8> {
        if self.fpga_flash.is_empty() {
            self.fpga_flash = vec![0xFF; FPGA_FLASH_BYTES as usize];
        }
        &mut self.fpga_flash
    }

    fn flash_reg24(&self, lo: u16, hi: u16) -> u32 {
        (u32::from(self.fpga_flash_reg[hi as usize] & 0xFF) << 16)
            | u32::from(self.fpga_flash_reg[lo as usize])
    }

    fn run_flash_command(&mut self, op: u8) {
        let addr = self.flash_reg24(ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1);
        let len = self.flash_reg24(ADDR_FLASH_LEN_0, ADDR_FLASH_LEN_1);
        self.fpga_flash_ops.push((op, addr, len));
        let end = (addr + len) as usize;
        let mut err = FlashErr::None;
        let mut result = 0u32;
        match FlashOp::from_u8(op) {
            Some(FlashOp::Crc32) => {
                let flash = self.fpga_flash_mut();
                result = crc32(&flash[addr as usize..end]);
            }
            Some(FlashOp::Erase) => {
                assert!(addr >= FPGA_GOLDEN_REGION_END, "erase of the golden image");
                assert!(end <= FPGA_FLASH_BYTES as usize);
                let first = addr - addr % FPGA_SECTOR_BYTES;
                let last = end.div_ceil(FPGA_SECTOR_BYTES as usize);
                self.fpga_flash_mut()[first as usize..last * FPGA_SECTOR_BYTES as usize].fill(0xFF);
            }
            Some(FlashOp::Program) => {
                assert!(
                    addr >= FPGA_GOLDEN_REGION_END,
                    "write into the golden image"
                );
                assert!(len as usize <= FLASH_BUF_BYTES);
                let data: Vec<u8> = self
                    .fpga_flash_buf
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .collect();
                let flash = self.fpga_flash_mut();
                for (cell, byte) in flash[addr as usize..end].iter_mut().zip(data) {
                    *cell &= byte;
                }
            }
            Some(FlashOp::Reboot) if self.fpga_reboots_to_ignore > 0 => {
                self.fpga_reboots_to_ignore -= 1;
            }
            Some(FlashOp::Reboot) => {
                self.fpga_reboots += 1;
                self.fpga_flash_reg = [0; 16];
                return;
            }
            _ => err = FlashErr::Invalid,
        }
        if let Some(injected) = self.fpga_flash_err {
            err = injected;
        }
        self.fpga_flash_reg[ADDR_FLASH_STATUS as usize] =
            (u16::from(err.as_u8()) << 8) | u16::from(self.fpga_flash_hang);
        self.fpga_flash_reg[ADDR_FLASH_RESULT_0 as usize] = result as u16;
        self.fpga_flash_reg[ADDR_FLASH_RESULT_1 as usize] = (result >> 16) as u16;
    }

    fn fire_isr_frame(&mut self) {
        let Some((cpu, seq, cmd)) = self.isr_frame.take() else {
            return;
        };
        self.isr_seen_reply = Some(cpu.reply());
        let _ = cpu.recv_frame(&[seq, cmd], 0);
    }
}

impl Port for MockPort {
    fn fpga_write(&mut self, addr: u16, value: u16) {
        self.fire_isr_frame();
        let select = (addr >> 8) as u8;
        let a = addr & 0x3FFF;
        match select >> 6 {
            sel if sel == BramSelect::Mod.as_u8() >> 6 => {
                let bank = self.ctl[ADDR_MOD_MEM_WR_BANK as usize] as usize;
                let page = self.ctl[ADDR_MOD_MEM_WR_PAGE as usize] as usize;
                self.mod_ram[bank][(page << 14) | a as usize] = value;
            }
            sel if sel == BramSelect::Emission.as_u8() >> 6 => {
                let bank = self.ctl[ADDR_PATTERN_MEM_WR_BANK as usize] as usize;
                let page = self.ctl[ADDR_PATTERN_MEM_WR_PAGE as usize] as usize;
                self.em_ram[bank][(page << 14) | a as usize] = value;
            }
            _ => self.write_controller(addr, value),
        }
    }

    fn fpga_read(&mut self, addr: u16) -> u16 {
        let select = (addr >> 8) as u8;
        if select == BramSelect::Controller.as_u8() {
            if addr & 0xFF == ADDR_CTL_FLAG {
                self.ctl_flag_reads += 1;
            }
            return self.ctl[(addr & 0xFF) as usize];
        }
        if select == BramSelect::Flash.as_u8() {
            if addr & 0xFF == ADDR_FLASH_STATUS {
                self.fpga_flash_status_reads += 1;
            }
            return match addr & 0xFF {
                ADDR_FLASH_USR_ACCESS_0 => self.fpga_usr_access as u16,
                ADDR_FLASH_USR_ACCESS_1 => (self.fpga_usr_access >> 16) as u16,
                r if r < 16 => self.fpga_flash_reg[r as usize],
                _ => 0,
            };
        }
        0
    }

    fn memory_barrier(&mut self) {
        self.fpga_flash_target_unflushed = false;
    }

    fn next_sync_edge(&mut self, guard_ns: u32) -> Option<NonZeroU64> {
        self.sync_guard_ns = Some(guard_ns);
        if let Some(next_sync_edge) = self.next_sync_edges.pop_front() {
            self.next_sync_edge = next_sync_edge;
        }
        self.next_sync_edge
    }

    fn configure_ptp(&mut self, config: autd3_cpu_wire::config::PtpConfig) {
        self.ptp_config = Some(config);
    }

    fn set_fpga_bus_wait(&mut self, wait: autd3_cpu_wire::config::FpgaBusWait) {
        self.fpga_bus_wait = Some(wait);
    }

    fn sys_time(&mut self) -> Option<u64> {
        if let Some(sys_time) = self.sys_time_reads.pop_front() {
            self.sys_time = sys_time;
        }
        self.sys_time
    }

    fn host_idle_ms(&mut self) -> Option<u32> {
        self.host_idle_ms
    }

    fn ptp_unlocked_ms(&mut self) -> Option<u32> {
        self.ptp_unlocked_ms
    }

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        if self.flash_fail {
            return Err(FlashError);
        }
        let start = addr as usize;
        buf.copy_from_slice(&self.flash[start..][..buf.len()]);
        Ok(())
    }

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        if self.flash_fail {
            return Err(FlashError);
        }
        if let Some(left) = self.flash_write_fail_after {
            if left == 0 {
                return Err(FlashError);
            }
            self.flash_write_fail_after = Some(left - 1);
        }
        assert!(addr >= LOADER_REGION_END, "write into the loader region");
        if self.flash_write_silent {
            return Ok(());
        }
        let start = addr as usize;
        for (cell, &byte) in self.flash[start..][..data.len()].iter_mut().zip(data) {
            *cell &= byte;
        }
        Ok(())
    }

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError> {
        if self.flash_fail {
            return Err(FlashError);
        }
        assert!(addr >= LOADER_REGION_END, "erase of the loader region");
        assert_eq!(addr % FLASH_SECTOR_BYTES, 0);
        assert_eq!(len % FLASH_SECTOR_BYTES, 0);
        self.erased.push((addr, len));
        let start = addr as usize;
        self.flash[start..][..len as usize].fill(0xFF);
        Ok(())
    }

    fn reset(&mut self) {
        self.reset_count += 1;
    }
}

pub(crate) struct Frame {
    seq: u8,
    cmd: u8,
    len: usize,
    payload: Box<[u8; PAYLOAD_BYTES]>,
}

impl Frame {
    pub(crate) fn new(seq: u8, cmd: Cmd) -> Self {
        Self::raw(seq, cmd as u8)
    }

    pub(crate) fn raw(seq: u8, cmd: u8) -> Self {
        Self {
            seq,
            cmd,
            len: 0,
            payload: Box::new([0; PAYLOAD_BYTES]),
        }
    }

    pub(crate) fn from_payload<P: IntoBytes + Immutable + ?Sized>(
        seq: u8,
        cmd: Cmd,
        payload: &P,
    ) -> Self {
        let mut f = Self::new(seq, cmd);
        let bytes = payload.as_bytes();
        f.payload[..bytes.len()].copy_from_slice(bytes);
        f.len = bytes.len();
        f
    }

    pub(crate) fn from_parts<H: IntoBytes + Immutable>(
        seq: u8,
        cmd: Cmd,
        header: &H,
        data: &[u8],
    ) -> Self {
        let mut f = Self::from_payload(seq, cmd, header);
        let h = core::mem::size_of::<H>();
        f.payload[h..][..data.len()].copy_from_slice(data);
        f.len = h + data.len();
        f
    }

    pub(crate) fn set_payload_byte(&mut self, index: usize, value: u8) {
        self.payload[index] = value;
        self.len = self.len.max(index + 1);
    }

    pub(crate) fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    pub(crate) fn bytes(&self) -> std::vec::Vec<u8> {
        let mut bytes = std::vec::Vec::with_capacity(size_of::<FrameHeader>() + self.len);
        bytes.extend_from_slice(
            FrameHeader {
                seq: self.seq,
                cmd: self.cmd,
            }
            .as_bytes(),
        );
        bytes.extend_from_slice(&self.payload[..self.len]);
        bytes
    }
}

pub(crate) struct Harness {
    pub cpu: Rc<Cpu>,
    pub port: MockPort,
}

impl Harness {
    pub(crate) fn new() -> Self {
        let cpu = Rc::new(Cpu::new());
        let mut port = MockPort::new();
        cpu.init(&mut port);
        Self { cpu, port }
    }

    pub(crate) fn reboot(&mut self) {
        self.cpu = Rc::new(Cpu::new());
        self.cpu.mark_boot_attempt(&mut self.port);
        self.cpu.init(&mut self.port);
    }

    pub(crate) fn deliver(&mut self, frame: &Frame) {
        self.deliver_no_drain(frame);
        self.cpu.process_pending(&mut self.port);
    }

    pub(crate) fn deliver_no_drain(&mut self, frame: &Frame) {
        let _ = self.cpu.recv_frame(&frame.bytes()[..], 0);
    }

    pub(crate) fn process_one(&mut self) -> bool {
        let mut work = RxFrame::ZERO;
        self.cpu.process_one(&mut self.port, &mut work) != Drained::Empty
    }

    fn reply(&self) -> Reply {
        self.cpu.reply()
    }

    pub(crate) fn ack(&self) -> u8 {
        self.reply().ack
    }

    pub(crate) fn status(&self) -> Error {
        self.reply().status
    }

    pub(crate) fn reply_data(&self) -> Vec<u8> {
        self.reply().data().to_vec()
    }

    pub(crate) fn firmware_info(&self) -> autd3_cpu_wire::payload::FirmwareInfo {
        use zerocopy::FromBytes;
        autd3_cpu_wire::payload::FirmwareInfo::read_from_bytes(&self.reply_data()[..])
            .expect("a firmware info reply")
    }

    pub(crate) fn expected_seq(&self) -> u8 {
        self.cpu.expected_seq()
    }

    pub(crate) fn ctl(&self, addr: u16) -> u16 {
        self.port.ctl[(addr & 0xFF) as usize]
    }

    pub(crate) fn latched(&self, addr: u16) -> u16 {
        self.port.latched[(addr & 0xFF) as usize]
    }

    pub(crate) fn set_ctl(&mut self, addr: u16, value: u16) {
        self.port.ctl[(addr & 0xFF) as usize] = value;
    }

    pub(crate) fn ctl_flags(&self) -> CtlFlags {
        CtlFlags::from_bits_retain(self.ctl(ADDR_CTL_FLAG))
    }

    pub(crate) fn latch_count(&self, flag: CtlFlags) -> u32 {
        self.port.latch_count_of(flag)
    }

    pub(crate) fn mod_word(&self, bank: u8, idx: usize) -> u16 {
        self.port.mod_ram[bank as usize][idx]
    }

    pub(crate) fn emission_word(&self, bank: u8, idx: usize) -> u16 {
        self.port.em_ram[bank as usize][idx]
    }

    pub(crate) fn arm_isr_frame(&mut self, seq: u8, cmd: Cmd) {
        self.port.isr_frame = Some((Rc::clone(&self.cpu), seq, cmd as u8));
    }

    pub(crate) fn telemetry(&self, id: Telemetry) -> u32 {
        self.cpu.telemetry(id)
    }

    pub(crate) fn output_mask(&self, idx: usize) -> u16 {
        self.port.output_mask[idx]
    }

    pub(crate) fn step_backlogged(&mut self, seq: &mut u8, elapsed_ms: u32) {
        self.deliver_no_drain(&Frame::new(*seq, Cmd::Nop));
        *seq = seq.wrapping_add(1);
        let mut work = RxFrame::ZERO;
        assert_ne!(
            self.cpu.step(&mut self.port, &mut work, elapsed_ms),
            Drained::Empty
        );
    }

    pub(crate) fn tick_1ms(&mut self, count: u32) {
        for _ in 0..count {
            self.cpu.tick_1ms(&mut self.port);
        }
    }
}
