#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_possible_wrap
)]

mod foci;
mod silencer;
mod swapchain;

use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::ffi;

pub use silencer::SilencerEmulator;
use swapchain::Swapchain;

const NUM_BANKS: usize = ffi::NUM_BANKS as usize;
const OUTPUT_MASK_WORDS: usize = ffi::OUTPUT_MASK_WORDS as usize;
const PWE_TABLE_SIZE: usize = ffi::PWE_TABLE_SIZE as usize;
const EMISSION_SLOT_WORDS: usize = ffi::EMISSION_SLOT_WORDS as usize;
const EMISSION_RAM_WORDS: usize = ffi::EMISSION_RAM_WORDS as usize;
const MOD_RAM_WORDS: usize = (ffi::MOD_BUFFER_SAMPLES / 2) as usize;

const SELECT_CONTROLLER: u16 = ffi::BRAM_SELECT_CONTROLLER as u16;
const SELECT_MOD: u16 = ffi::BRAM_SELECT_MOD as u16;
const SELECT_PWE_TABLE: u16 = ffi::BRAM_SELECT_PWE_TABLE as u16;
const SELECT_EMISSION: u16 = ffi::BRAM_SELECT_EMISSION as u16;

const CNT_SELECT_MAIN: usize = ffi::BRAM_CNT_SELECT_MAIN as usize;
const CNT_SELECT_PHASE_CORR: usize = ffi::BRAM_CNT_SELECT_PHASE_CORR as usize;
const CNT_SELECT_OUTPUT_MASK: usize = ffi::BRAM_CNT_SELECT_OUTPUT_MASK as usize;

const LATCH_MASK: u16 = (ffi::CTL_FLAG_MOD_SET
    | ffi::CTL_FLAG_PATTERN_SET
    | ffi::CTL_FLAG_SILENCER_SET
    | ffi::CTL_FLAG_DEBUG_SET
    | ffi::CTL_FLAG_SYNC_SET) as u16;

const CTL_FLAG_MOD_SET: u16 = ffi::CTL_FLAG_MOD_SET as u16;
const CTL_FLAG_PATTERN_SET: u16 = ffi::CTL_FLAG_PATTERN_SET as u16;
const SILENCER_FIXED_UPDATE_RATE_MODE: u16 = ffi::SILENCER_FLAG_FIXED_UPDATE_RATE_MODE as u16;

const fn reg(a: u32) -> usize {
    a as usize
}

pub struct FpgaEmulator {
    num_transducers: usize,
    controller: Box<[u16; 256]>,
    phase_corr: Box<[u16; 256]>,
    output_mask: Box<[u16; OUTPUT_MASK_WORDS]>,
    pwe: Box<[u16; PWE_TABLE_SIZE]>,
    mod_ram: Vec<Box<[u16]>>,
    em_ram: Vec<Box<[u16]>>,
    latch_count: [u32; 16],
    next_sync0: u64,
    sys_time_ns: u64,
    gpio_in: [bool; 4],
    mod_swapchain: Swapchain,
    pattern_swapchain: Swapchain,
}

impl FpgaEmulator {
    #[must_use]
    pub(crate) fn new(num_transducers: usize) -> Self {
        let mut controller = Box::new([0u16; 256]);

        controller[reg(ffi::ADDR_VERSION_NUM_MAJOR)] = ffi::VERSION_NUM_MAJOR as u16;
        controller[reg(ffi::ADDR_VERSION_NUM_MINOR)] = ffi::VERSION_NUM_MINOR as u16;
        controller[reg(ffi::ADDR_VERSION_NUM_PATCH)] = ffi::VERSION_NUM_PATCH as u16;
        Self {
            num_transducers,
            controller,
            phase_corr: Box::new([0u16; 256]),
            output_mask: Box::new([0u16; OUTPUT_MASK_WORDS]),
            pwe: Box::new([0u16; PWE_TABLE_SIZE]),
            mod_ram: (0..NUM_BANKS)
                .map(|_| vec![0u16; MOD_RAM_WORDS].into_boxed_slice())
                .collect(),
            em_ram: (0..NUM_BANKS)
                .map(|_| vec![0u16; EMISSION_RAM_WORDS].into_boxed_slice())
                .collect(),
            latch_count: [0; 16],
            next_sync0: 0,
            sys_time_ns: 0,
            gpio_in: [false; 4],
            mod_swapchain: Swapchain::new(),
            pattern_swapchain: Swapchain::new(),
        }
    }

    pub(crate) fn write(&mut self, addr: u16, value: u16) {
        let select = (addr >> 14) & 0x3;
        let a = (addr & 0x3FFF) as usize;
        match select {
            SELECT_CONTROLLER => self.write_controller(a, value),
            SELECT_MOD => {
                let bank = self.controller[reg(ffi::ADDR_MOD_MEM_WR_BANK)] as usize;
                let page = self.controller[reg(ffi::ADDR_MOD_MEM_WR_PAGE)] as usize;
                self.mod_ram[bank][(page << 14) | a] = value;
            }
            SELECT_PWE_TABLE => self.pwe[a & (PWE_TABLE_SIZE - 1)] = value,
            SELECT_EMISSION => {
                let bank = self.controller[reg(ffi::ADDR_PATTERN_MEM_WR_BANK)] as usize;
                let page = self.controller[reg(ffi::ADDR_PATTERN_MEM_WR_PAGE)] as usize;
                self.em_ram[bank][(page << 14) | a] = value;
            }
            _ => {}
        }
    }

    fn write_controller(&mut self, a: usize, value: u16) {
        match a >> 8 {
            CNT_SELECT_MAIN => {
                if a == reg(ffi::ADDR_CTL_FLAG) {
                    for bit in 0..16 {
                        if value & LATCH_MASK & (1 << bit) != 0 {
                            self.latch_count[bit] += 1;
                        }
                    }
                    self.controller[reg(ffi::ADDR_CTL_FLAG)] = value & !LATCH_MASK;
                    if value & CTL_FLAG_MOD_SET != 0 {
                        self.arm_mod_swapchain();
                    }
                    if value & CTL_FLAG_PATTERN_SET != 0 {
                        self.arm_pattern_swapchain();
                    }
                } else {
                    self.controller[a & 0xFF] = value;
                }
            }
            CNT_SELECT_PHASE_CORR => self.phase_corr[a & 0xFF] = value,
            CNT_SELECT_OUTPUT_MASK => self.output_mask[a & (OUTPUT_MASK_WORDS - 1)] = value,
            _ => {}
        }
    }

    pub(crate) fn read(&self, addr: u16) -> u16 {
        let select = (addr >> 14) & 0x3;
        let a = (addr & 0x3FFF) as usize;
        if select == SELECT_CONTROLLER && (a >> 8) == CNT_SELECT_MAIN {
            self.controller[a & 0xFF]
        } else {
            0
        }
    }

    pub(crate) fn next_sync0(&mut self) -> u64 {
        self.next_sync0
    }

    fn reg_u64(&self, base: usize) -> u64 {
        (0..4)
            .map(|i| u64::from(self.controller[base + i]) << (16 * i))
            .sum()
    }

    fn arm_mod_swapchain(&mut self) {
        let req = self.controller[reg(ffi::ADDR_MOD_REQ_RD_BANK)] as usize;
        let rep = self.controller[reg(ffi::ADDR_MOD_REP0) + req];
        let freq_div = self.controller[reg(ffi::ADDR_MOD_FREQ_DIV0) + req];
        let cycle = self.modulation_cycle(req);
        let mode = self.controller[reg(ffi::ADDR_MOD_TRANSITION_MODE)] as u8;
        let value = self.reg_u64(reg(ffi::ADDR_MOD_TRANSITION_VALUE_0));
        self.mod_swapchain
            .set(self.sys_time_ns, rep, freq_div, cycle, req, mode, value);
    }

    fn arm_pattern_swapchain(&mut self) {
        let req = self.controller[reg(ffi::ADDR_PATTERN_REQ_RD_BANK)] as usize;
        let rep = self.controller[reg(ffi::ADDR_PATTERN_REP0) + req];
        let freq_div = self.controller[reg(ffi::ADDR_PATTERN_FREQ_DIV0) + req];
        let cycle = self.pattern_cycle(req);
        let mode = self.controller[reg(ffi::ADDR_PATTERN_TRANSITION_MODE)] as u8;
        let value = self.reg_u64(reg(ffi::ADDR_PATTERN_TRANSITION_VALUE_0));
        self.pattern_swapchain
            .set(self.sys_time_ns, rep, freq_div, cycle, req, mode, value);
    }

    pub fn update_with_sys_time(&mut self, sys_time_ns: u64) {
        self.sys_time_ns = sys_time_ns;
        self.mod_swapchain.update(self.gpio_in, sys_time_ns);
        self.pattern_swapchain.update(self.gpio_in, sys_time_ns);
    }

    pub fn set_next_sync0(&mut self, sys_time_ns: u64) {
        self.next_sync0 = sys_time_ns;
    }

    pub fn set_gpio_in(&mut self, gpio_in: [bool; 4]) {
        self.gpio_in = gpio_in;
    }

    #[must_use]
    pub fn current_mod_bank(&self) -> usize {
        self.mod_swapchain.cur_bank()
    }

    #[must_use]
    pub fn current_mod_idx(&self) -> usize {
        self.mod_swapchain.cur_idx()
    }

    #[must_use]
    pub fn current_pattern_bank(&self) -> usize {
        self.pattern_swapchain.cur_bank()
    }

    #[must_use]
    pub fn current_pattern_idx(&self) -> usize {
        self.pattern_swapchain.cur_idx()
    }

    #[must_use]
    pub fn drives(&self) -> Vec<Emission> {
        let bank = self.current_pattern_bank();
        let idx = self.current_pattern_idx();
        if self.pattern_mode(bank) == ffi::EMISSION_TYPE_FOCI as u16 {
            self.foci_drives_at(bank, idx)
        } else {
            self.drives_at(bank, idx)
        }
    }

    #[must_use]
    pub fn modulation(&self) -> u8 {
        self.modulation_at(self.current_mod_bank(), self.current_mod_idx())
    }

    #[must_use]
    pub fn silencer_fixed_update_rate_mode(&self) -> bool {
        self.controller[reg(ffi::ADDR_SILENCER_FLAG)] & SILENCER_FIXED_UPDATE_RATE_MODE != 0
    }

    #[must_use]
    pub fn silencer_update_rate_intensity(&self) -> u16 {
        self.controller[reg(ffi::ADDR_SILENCER_UPDATE_RATE_INTENSITY)]
    }

    #[must_use]
    pub fn silencer_update_rate_phase(&self) -> u16 {
        self.controller[reg(ffi::ADDR_SILENCER_UPDATE_RATE_PHASE)]
    }

    #[must_use]
    pub fn silencer_completion_steps_intensity(&self) -> u16 {
        self.controller[reg(ffi::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY)]
    }

    #[must_use]
    pub fn silencer_completion_steps_phase(&self) -> u16 {
        self.controller[reg(ffi::ADDR_SILENCER_COMPLETION_STEPS_PHASE)]
    }

    #[must_use]
    pub fn silencer_emulator_phase(&self, initial: u8) -> SilencerEmulator {
        let fixed_rate = self.silencer_fixed_update_rate_mode();
        let value = if fixed_rate {
            self.silencer_update_rate_phase()
        } else {
            self.silencer_completion_steps_phase()
        };
        SilencerEmulator::new(true, initial, fixed_rate, value)
    }

    #[must_use]
    pub fn silencer_emulator_intensity(&self, initial: u8) -> SilencerEmulator {
        let fixed_rate = self.silencer_fixed_update_rate_mode();
        let value = if fixed_rate {
            self.silencer_update_rate_intensity()
        } else {
            self.silencer_completion_steps_intensity()
        };
        SilencerEmulator::new(false, initial, fixed_rate, value)
    }

    #[must_use]
    pub fn num_transducers(&self) -> usize {
        self.num_transducers
    }

    #[must_use]
    pub fn fpga_version(&self) -> (u16, u16, u16) {
        (
            self.controller[reg(ffi::ADDR_VERSION_NUM_MAJOR)],
            self.controller[reg(ffi::ADDR_VERSION_NUM_MINOR)],
            self.controller[reg(ffi::ADDR_VERSION_NUM_PATCH)],
        )
    }

    #[must_use]
    pub fn controller_reg(&self, addr: u16) -> u16 {
        self.controller[addr as usize & 0xFF]
    }

    #[must_use]
    pub fn latch_count(&self, flag: u16) -> u32 {
        (0..16)
            .find(|bit| flag & (1 << bit) != 0)
            .map_or(0, |bit| self.latch_count[bit])
    }

    #[must_use]
    pub fn req_modulation_bank(&self) -> u16 {
        self.controller[reg(ffi::ADDR_MOD_REQ_RD_BANK)]
    }

    #[must_use]
    pub fn req_pattern_bank(&self) -> u16 {
        self.controller[reg(ffi::ADDR_PATTERN_REQ_RD_BANK)]
    }

    #[must_use]
    pub fn modulation_cycle(&self, bank: usize) -> usize {
        self.controller[reg(ffi::ADDR_MOD_CYCLE0) + bank] as usize + 1
    }

    #[must_use]
    pub fn modulation_freq_div(&self, bank: usize) -> u16 {
        self.controller[reg(ffi::ADDR_MOD_FREQ_DIV0) + bank]
    }

    #[must_use]
    pub fn pattern_cycle(&self, bank: usize) -> usize {
        self.controller[reg(ffi::ADDR_PATTERN_CYCLE0) + bank] as usize + 1
    }

    #[must_use]
    pub fn pattern_freq_div(&self, bank: usize) -> u16 {
        self.controller[reg(ffi::ADDR_PATTERN_FREQ_DIV0) + bank]
    }

    #[must_use]
    pub fn pattern_mode(&self, bank: usize) -> u16 {
        self.controller[reg(ffi::ADDR_PATTERN_MODE0) + bank]
    }

    #[must_use]
    pub fn modulation_at(&self, bank: usize, idx: usize) -> u8 {
        let word = self.mod_ram[bank][idx >> 1];
        if idx.is_multiple_of(2) {
            (word & 0xFF) as u8
        } else {
            (word >> 8) as u8
        }
    }

    #[must_use]
    pub fn modulation_buffer(&self, bank: usize) -> Vec<u8> {
        (0..self.modulation_cycle(bank))
            .map(|i| self.modulation_at(bank, i))
            .collect()
    }

    #[must_use]
    pub fn phase_correction(&self, i: usize) -> Phase {
        let word = self.phase_corr[i >> 1];
        let v = if i.is_multiple_of(2) {
            word & 0xFF
        } else {
            word >> 8
        };
        Phase(v as u8)
    }

    #[must_use]
    pub fn output_mask_enabled(&self, i: usize) -> bool {
        self.output_mask[i >> 4] & (1 << (i & 0x0F)) != 0
    }

    #[must_use]
    pub fn to_pulse_width(&self, intensity: Intensity, modulation: u8) -> u16 {
        let key = (intensity.0 as usize * modulation as usize) / 255;
        self.pwe[key & (PWE_TABLE_SIZE - 1)]
    }

    #[must_use]
    pub fn drives_at(&self, bank: usize, idx: usize) -> Vec<Emission> {
        let base = idx * EMISSION_SLOT_WORDS;
        (0..self.num_transducers)
            .map(|i| {
                let word = self.em_ram[bank][base + i];
                let phase = Phase((word & 0xFF) as u8) + self.phase_correction(i);
                let intensity = if self.output_mask_enabled(i) {
                    Intensity((word >> 8) as u8)
                } else {
                    Intensity::MIN
                };
                Emission { phase, intensity }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(fpga: &mut FpgaEmulator, addr: u32, value: u16) {
        fpga.write(addr as u16, value);
    }

    #[test]
    fn silencer_fixed_update_rate_intensity() {
        let mut fpga = FpgaEmulator::new(249);
        write(
            &mut fpga,
            ffi::ADDR_SILENCER_FLAG,
            SILENCER_FIXED_UPDATE_RATE_MODE,
        );
        write(&mut fpga, ffi::ADDR_SILENCER_UPDATE_RATE_INTENSITY, 1);
        let mut s = fpga.silencer_emulator_intensity(0);
        let out: Vec<u8> = (0..256).map(|_| s.apply(1)).collect();
        let mut expect = vec![0u8; 255];
        expect.push(1);
        assert_eq!(expect, out);
    }

    #[test]
    fn silencer_completion_steps_intensity() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, ffi::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, 10);
        let mut s = fpga.silencer_emulator_intensity(10);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(vec![21, 33, 45, 57, 69, 80, 92, 104, 116, 128, 128], out);
    }

    #[test]
    fn silencer_completion_steps_phase() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, ffi::ADDR_SILENCER_COMPLETION_STEPS_PHASE, 10);
        let mut s = fpga.silencer_emulator_phase(0);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(vec![12, 25, 38, 51, 64, 76, 89, 102, 115, 128, 128], out);
    }

    #[test]
    fn silencer_phase_wraps_shortest_path() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, ffi::ADDR_SILENCER_COMPLETION_STEPS_PHASE, 10);
        let mut s = fpga.silencer_emulator_phase(180);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(
            vec![174, 169, 164, 159, 153, 148, 143, 138, 133, 128, 128],
            out
        );
    }
}
