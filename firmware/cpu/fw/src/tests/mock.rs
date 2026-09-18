use core::cell::Cell;
use std::boxed::Box;
use std::rc::Rc;
use std::vec;
use std::vec::Vec;

use zerocopy::{Immutable, IntoBytes};

use crate::app::Cpu;
use crate::fpga::PWE_TABLE_SIZE;
use crate::params::{
    ADDR_CTL_FLAG, ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1, ADDR_FLASH_CMD, ADDR_FLASH_LEN_0,
    ADDR_FLASH_LEN_1, ADDR_FLASH_RESULT_0, ADDR_FLASH_RESULT_1, ADDR_FLASH_STATUS,
    ADDR_FLASH_USR_ACCESS_0, ADDR_FLASH_USR_ACCESS_1, ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE,
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_CNT_SELECT_FLASH,
    BRAM_CNT_SELECT_FLASH_BUF, BRAM_CNT_SELECT_MAIN, BRAM_CNT_SELECT_OUTPUT_MASK,
    BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER, BRAM_SELECT_EMISSION, BRAM_SELECT_MOD,
    BRAM_SELECT_PWE_TABLE, CTL_FLAG_DEBUG_SET, CTL_FLAG_MOD_SET, CTL_FLAG_PATTERN_SET,
    CTL_FLAG_SILENCER_SET, CTL_FLAG_SYNC_SET, FLASH_BUF_BYTES, FLASH_ERR_INVALID, FLASH_ERR_NONE,
    FLASH_OP_CRC32, FLASH_OP_ERASE, FLASH_OP_PROGRAM, FLASH_OP_READ_ID, FLASH_OP_REBOOT, NUM_BANKS,
};
use crate::port::{FlashError, Port};
use crate::proto::{
    Cmd, EMISSION_RAM_WORDS, MOD_BUFFER_SAMPLES, OUTPUT_MASK_WORDS, PAYLOAD_BYTES, Telemetry,
    TxFrame, WIRE_RX_FRAME_BYTES, WIRE_RX_GAP_END, WIRE_RX_GAP_START,
};
use autd3_cpu_wire::fpga_update::{FPGA_FLASH_BYTES, FPGA_GOLDEN_REGION_END, FPGA_SECTOR_BYTES};
use autd3_cpu_wire::update::{FLASH_BYTES, FLASH_SECTOR_BYTES, LOADER_REGION_END, crc32};

pub(crate) const MOD_RAM_WORDS: usize = (MOD_BUFFER_SAMPLES / 2) as usize;
pub(crate) const EM_RAM_WORDS: usize = EMISSION_RAM_WORDS as usize;

const LATCH_MASK: u16 = CTL_FLAG_MOD_SET
    | CTL_FLAG_PATTERN_SET
    | CTL_FLAG_SILENCER_SET
    | CTL_FLAG_DEBUG_SET
    | CTL_FLAG_SYNC_SET;

struct IsrPort {
    published_tx: Rc<Cell<Option<TxFrame>>>,
}

impl Port for IsrPort {
    fn fpga_write(&mut self, _addr: u16, _value: u16) {}
    fn fpga_read(&mut self, _addr: u16) -> u16 {
        0
    }
    fn memory_barrier(&mut self) {}
    fn next_sync0(&mut self) -> u64 {
        0
    }
    fn dc_sys_time(&mut self) -> u64 {
        0
    }
    fn sync0_cycle_ns(&mut self) -> u32 {
        0
    }
    fn al_status_code(&mut self) -> u16 {
        0
    }
    fn publish_tx(&mut self, tx: TxFrame) {
        self.published_tx.set(Some(tx));
    }

    fn flash_read(&mut self, _addr: u32, _buf: &mut [u8]) -> Result<(), FlashError> {
        Err(FlashError)
    }

    fn flash_write(&mut self, _addr: u32, _data: &[u8]) -> Result<(), FlashError> {
        Err(FlashError)
    }

    fn flash_erase(&mut self, _addr: u32, _len: u32) -> Result<(), FlashError> {
        Err(FlashError)
    }

    fn reset(&mut self) {}
}

pub(crate) struct MockPort {
    pub ctl: Box<[u16; 256]>,
    pub phase_corr: Box<[u16; 256]>,
    pub output_mask: Box<[u16; OUTPUT_MASK_WORDS]>,
    pub pwe: Box<[u16; PWE_TABLE_SIZE]>,
    pub mod_ram: Vec<Vec<u16>>,
    pub em_ram: Vec<Vec<u16>>,
    pub latch_count: [u32; 16],
    pub next_sync0: u64,
    pub dc_sys_time: u64,
    pub sync0_cycle_ns: u32,
    pub al_status_code: u16,
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
    pub fpga_flash_err: Option<u8>,
    pub fpga_flash_hang: bool,
    pub fpga_flash_dropped_reg: Option<u16>,
    pub fpga_flash_target_unflushed: bool,
    pub fpga_flash_cmds_before_flush: u32,
    pub fpga_usr_access: u32,
    pub fpga_reboots: u32,
    pub fpga_reboots_to_ignore: u32,
    published_tx: Rc<Cell<Option<TxFrame>>>,
    isr_frame: Option<(Rc<Cpu>, u8, u8)>,
}

impl MockPort {
    pub(crate) fn new() -> Self {
        Self {
            ctl: Box::new([0; 256]),
            phase_corr: Box::new([0; 256]),
            output_mask: Box::new([0; OUTPUT_MASK_WORDS]),
            pwe: Box::new([0; PWE_TABLE_SIZE]),
            mod_ram: vec![vec![0; MOD_RAM_WORDS]; NUM_BANKS],
            em_ram: vec![vec![0; EM_RAM_WORDS]; NUM_BANKS],
            latch_count: [0; 16],
            next_sync0: 0,
            dc_sys_time: 0,
            sync0_cycle_ns: 1_000_000,
            al_status_code: 0,
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
            published_tx: Rc::new(Cell::new(None)),
            isr_frame: None,
        }
    }

    pub(crate) fn latch_count_of(&self, flag: u16) -> u32 {
        (0..16usize)
            .find(|bit| (flag & (1 << bit)) != 0)
            .map_or(0, |bit| self.latch_count[bit])
    }

    fn write_controller(&mut self, addr: u16, value: u16) {
        match (addr >> 8) as u8 {
            BRAM_CNT_SELECT_MAIN => {
                if addr == ADDR_CTL_FLAG {
                    for bit in 0..16usize {
                        if (value & LATCH_MASK & (1 << bit)) != 0 {
                            self.latch_count[bit] += 1;
                        }
                    }
                    self.ctl[ADDR_CTL_FLAG as usize] = if self.latch_stuck {
                        value
                    } else {
                        value & !LATCH_MASK
                    };
                } else {
                    self.ctl[(addr & 0xFF) as usize] = value;
                }
            }
            BRAM_CNT_SELECT_FLASH if self.fpga_flash_dropped_reg == Some(addr & 0xFF) => {}
            BRAM_CNT_SELECT_FLASH => {
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
            sel if sel >> 1 == BRAM_CNT_SELECT_FLASH_BUF >> 1 => {
                self.fpga_flash_buf[(addr & 0x1FF) as usize] = value;
            }
            BRAM_CNT_SELECT_PHASE_CORR => self.phase_corr[(addr & 0xFF) as usize] = value,
            BRAM_CNT_SELECT_OUTPUT_MASK => {
                self.output_mask[(addr as usize) & (OUTPUT_MASK_WORDS - 1)] = value;
            }
            _ => {}
        }
    }

    pub(crate) fn fpga_flash_mut(&mut self) -> &mut Vec<u8> {
        if self.fpga_flash.is_empty() {
            self.fpga_flash = vec![0xFF; FPGA_FLASH_BYTES as usize];
        }
        &mut self.fpga_flash
    }

    fn flash_reg24(&self, lo: u16, hi: u16) -> u32 {
        u32::from(self.fpga_flash_reg[lo as usize])
            | (u32::from(self.fpga_flash_reg[hi as usize] & 0xFF) << 16)
    }

    fn run_flash_command(&mut self, op: u8) {
        let addr = self.flash_reg24(ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1);
        let len = self.flash_reg24(ADDR_FLASH_LEN_0, ADDR_FLASH_LEN_1);
        self.fpga_flash_ops.push((op, addr, len));
        let end = (addr + len) as usize;
        let mut err = FLASH_ERR_NONE;
        let mut result = 0u32;
        match op {
            FLASH_OP_READ_ID => result = 0x0020_BA18,
            FLASH_OP_CRC32 => {
                let flash = self.fpga_flash_mut();
                result = crc32(&flash[addr as usize..end]);
            }
            FLASH_OP_ERASE => {
                assert!(addr >= FPGA_GOLDEN_REGION_END, "erase of the golden image");
                assert!(end <= FPGA_FLASH_BYTES as usize);
                let first = addr - addr % FPGA_SECTOR_BYTES;
                let last = end.div_ceil(FPGA_SECTOR_BYTES as usize);
                self.fpga_flash_mut()[first as usize..last * FPGA_SECTOR_BYTES as usize].fill(0xFF);
            }
            FLASH_OP_PROGRAM => {
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
            FLASH_OP_REBOOT if self.fpga_reboots_to_ignore > 0 => {
                self.fpga_reboots_to_ignore -= 1;
            }
            FLASH_OP_REBOOT => {
                self.fpga_reboots += 1;
                self.fpga_flash_reg = [0; 16];
                return;
            }
            _ => err = FLASH_ERR_INVALID,
        }
        if let Some(injected) = self.fpga_flash_err {
            err = injected;
        }
        self.fpga_flash_reg[ADDR_FLASH_STATUS as usize] =
            (u16::from(err) << 8) | u16::from(self.fpga_flash_hang);
        self.fpga_flash_reg[ADDR_FLASH_RESULT_0 as usize] = result as u16;
        self.fpga_flash_reg[ADDR_FLASH_RESULT_1 as usize] = (result >> 16) as u16;
    }

    fn fire_isr_frame(&mut self) {
        let Some((cpu, seq, cmd)) = self.isr_frame.take() else {
            return;
        };
        let mut wire = [0u8; WIRE_RX_FRAME_BYTES];
        wire[0] = seq;
        wire[1] = cmd;
        let mut isr_port = IsrPort {
            published_tx: Rc::clone(&self.published_tx),
        };
        cpu.recv_ethercat(&mut isr_port, &wire);
    }
}

impl Port for MockPort {
    fn fpga_write(&mut self, addr: u16, value: u16) {
        self.fire_isr_frame();
        let select = ((addr >> 14) & 0x3) as u8;
        let a = addr & 0x3FFF;
        match select {
            BRAM_SELECT_CONTROLLER => self.write_controller(a, value),
            BRAM_SELECT_MOD => {
                let bank = self.ctl[ADDR_MOD_MEM_WR_BANK as usize] as usize;
                let page = self.ctl[ADDR_MOD_MEM_WR_PAGE as usize] as usize;
                self.mod_ram[bank][(page << 14) | a as usize] = value;
            }
            BRAM_SELECT_PWE_TABLE => self.pwe[(addr as usize) & (PWE_TABLE_SIZE - 1)] = value,
            BRAM_SELECT_EMISSION => {
                let bank = self.ctl[ADDR_PATTERN_MEM_WR_BANK as usize] as usize;
                let page = self.ctl[ADDR_PATTERN_MEM_WR_PAGE as usize] as usize;
                self.em_ram[bank][(page << 14) | a as usize] = value;
            }
            _ => {}
        }
    }

    fn fpga_read(&mut self, addr: u16) -> u16 {
        let select = ((addr >> 14) & 0x3) as u8;
        let a = addr & 0x3FFF;
        if select == BRAM_SELECT_CONTROLLER && (a >> 8) as u8 == BRAM_CNT_SELECT_MAIN {
            return self.ctl[(a & 0xFF) as usize];
        }
        if select == BRAM_SELECT_CONTROLLER && (a >> 8) as u8 == BRAM_CNT_SELECT_FLASH {
            return match a & 0xFF {
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

    fn next_sync0(&mut self) -> u64 {
        self.next_sync0
    }

    fn dc_sys_time(&mut self) -> u64 {
        self.dc_sys_time
    }

    fn sync0_cycle_ns(&mut self) -> u32 {
        self.sync0_cycle_ns
    }

    fn al_status_code(&mut self) -> u16 {
        self.al_status_code
    }

    fn publish_tx(&mut self, tx: TxFrame) {
        self.fire_isr_frame();
        self.published_tx.set(Some(tx));
    }

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        if self.flash_fail {
            return Err(FlashError);
        }
        let start = addr as usize;
        buf.copy_from_slice(&self.flash[start..start + buf.len()]);
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
        for (cell, &byte) in self.flash[start..start + data.len()].iter_mut().zip(data) {
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
        self.flash[start..start + len as usize].fill(0xFF);
        Ok(())
    }

    fn reset(&mut self) {
        self.reset_count += 1;
    }
}

pub(crate) struct Frame {
    seq: u8,
    cmd: u8,
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
        f.payload[h..h + data.len()].copy_from_slice(data);
        f
    }

    pub(crate) fn wire(&self) -> Box<[u8; WIRE_RX_FRAME_BYTES]> {
        let mut wire = Box::new([0u8; WIRE_RX_FRAME_BYTES]);
        wire[0] = self.seq;
        wire[1] = self.cmd;
        let head = WIRE_RX_GAP_START - 2;
        wire[2..WIRE_RX_GAP_START].copy_from_slice(&self.payload[..head]);
        wire[WIRE_RX_GAP_END..].copy_from_slice(&self.payload[head..]);
        wire
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

    pub(crate) fn init(&mut self) {
        self.cpu.init(&mut self.port);
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
        self.cpu.recv_ethercat(&mut self.port, &frame.wire());
    }

    pub(crate) fn process_one(&mut self) -> bool {
        self.cpu.process_one(&mut self.port)
    }

    fn tx(&self) -> TxFrame {
        let tx = self.cpu.tx();
        assert_eq!(self.port.published_tx.get(), Some(tx));
        tx
    }

    pub(crate) fn ack(&self) -> u8 {
        self.tx().ack
    }

    pub(crate) fn data(&self) -> u8 {
        self.tx().data
    }

    pub(crate) fn expected_seq(&self) -> u8 {
        self.cpu.expected_seq()
    }

    pub(crate) fn ctl(&self, addr: u16) -> u16 {
        self.port.ctl[(addr & 0xFF) as usize]
    }

    pub(crate) fn set_ctl(&mut self, addr: u16, value: u16) {
        self.port.ctl[(addr & 0xFF) as usize] = value;
    }

    pub(crate) fn latch_count(&self, flag: u16) -> u32 {
        self.port.latch_count_of(flag)
    }

    pub(crate) fn mod_word(&self, bank: u8, idx: usize) -> u16 {
        self.port.mod_ram[bank as usize][idx]
    }

    pub(crate) fn emission_word(&self, bank: u8, idx: usize) -> u16 {
        self.port.em_ram[bank as usize][idx]
    }

    pub(crate) fn arm_isr_reset(&mut self) {
        self.arm_isr_frame(0, Cmd::Reset);
    }

    pub(crate) fn arm_isr_frame(&mut self, seq: u8, cmd: Cmd) {
        self.port.isr_frame = Some((Rc::clone(&self.cpu), seq, cmd as u8));
    }

    pub(crate) fn telemetry(&self, id: Telemetry) -> u8 {
        self.cpu.telemetry(id)
    }

    pub(crate) fn output_mask(&self, idx: usize) -> u16 {
        self.port.output_mask[idx]
    }

    pub(crate) fn tick_1ms(&mut self, count: u32) {
        for _ in 0..count {
            self.cpu.tick_1ms(&mut self.port);
        }
    }
}
