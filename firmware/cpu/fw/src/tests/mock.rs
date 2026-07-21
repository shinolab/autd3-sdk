use std::boxed::Box;
use std::rc::Rc;
use std::vec;
use std::vec::Vec;

use zerocopy::{Immutable, IntoBytes};

use crate::app::Cpu;
use crate::fpga::PWE_TABLE_SIZE;
use crate::params::{
    ADDR_CTL_FLAG, ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, ADDR_PATTERN_MEM_WR_BANK,
    ADDR_PATTERN_MEM_WR_PAGE, BRAM_CNT_SELECT_MAIN, BRAM_CNT_SELECT_OUTPUT_MASK,
    BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER, BRAM_SELECT_EMISSION, BRAM_SELECT_MOD,
    BRAM_SELECT_PWE_TABLE, CTL_FLAG_DEBUG_SET, CTL_FLAG_MOD_SET, CTL_FLAG_PATTERN_SET,
    CTL_FLAG_SILENCER_SET, CTL_FLAG_SYNC_SET, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{
    Cmd, EMISSION_RAM_WORDS, MOD_BUFFER_SAMPLES, OUTPUT_MASK_WORDS, PAYLOAD_BYTES, Telemetry,
    WIRE_RX_FRAME_BYTES, WIRE_RX_GAP_END, WIRE_RX_GAP_START,
};

pub(crate) const MOD_RAM_WORDS: usize = (MOD_BUFFER_SAMPLES / 2) as usize;
pub(crate) const EM_RAM_WORDS: usize = EMISSION_RAM_WORDS as usize;

const LATCH_MASK: u16 = CTL_FLAG_MOD_SET
    | CTL_FLAG_PATTERN_SET
    | CTL_FLAG_SILENCER_SET
    | CTL_FLAG_DEBUG_SET
    | CTL_FLAG_SYNC_SET;

struct NullPort;

impl Port for NullPort {
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
            BRAM_CNT_SELECT_PHASE_CORR => self.phase_corr[(addr & 0xFF) as usize] = value,
            BRAM_CNT_SELECT_OUTPUT_MASK => {
                self.output_mask[(addr as usize) & (OUTPUT_MASK_WORDS - 1)] = value;
            }
            _ => {}
        }
    }

    fn fire_isr_frame(&mut self) {
        let Some((cpu, seq, cmd)) = self.isr_frame.take() else {
            return;
        };
        let mut wire = [0u8; WIRE_RX_FRAME_BYTES];
        wire[0] = seq;
        wire[1] = cmd;
        cpu.recv_ethercat(&mut NullPort, &wire);
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
        0
    }

    fn memory_barrier(&mut self) {}

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

    pub(crate) fn ack(&self) -> u8 {
        self.cpu.tx().ack
    }

    pub(crate) fn data(&self) -> u8 {
        self.cpu.tx().data
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
