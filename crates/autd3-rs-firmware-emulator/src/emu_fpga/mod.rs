#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_possible_wrap
)]

mod flash;
mod foci;
mod silencer;
mod swapchain;

use autd3_rs_core::value::{Intensity, Phase};

use crate::fw;
use crate::fw::{
    CtlFlags, FpgaStateFlags, FunctionBits, NUM_BANKS, OUTPUT_MASK_WORDS, PWE_TABLE_SIZE,
};
use autd3_cpu_fw::update::{FLASH_BYTES, IMAGE_MAGIC, Slot, crc32, is_plausible_length};

pub const EMULATED_CPU_IMAGE: &[u8] =
    b"autd3-rs-firmware-emulator: a slot-A image long enough to carry a vector block and then some";

const _: () = assert!(is_plausible_length(EMULATED_CPU_IMAGE.len() as u32));

fn fresh_cpu_flash() -> Box<[u8]> {
    let mut flash = vec![0xFF; FLASH_BYTES as usize].into_boxed_slice();
    let header_at = Slot::A.base() as usize;
    let body_at = Slot::A.image_base() as usize;
    let header = [
        IMAGE_MAGIC,
        0,
        EMULATED_CPU_IMAGE.len() as u32,
        crc32(EMULATED_CPU_IMAGE),
    ];
    for (i, word) in header.iter().enumerate() {
        flash[header_at + 4 * i..][..4].copy_from_slice(&word.to_le_bytes());
    }
    flash[body_at..][..EMULATED_CPU_IMAGE.len()].copy_from_slice(EMULATED_CPU_IMAGE);
    flash
}

pub use silencer::SilencerEmulator;
use swapchain::Swapchain;

const EMISSION_SLOT_WORDS: usize = fw::EMISSION_SLOT_WORDS as usize;
const EMISSION_RAM_WORDS: usize = fw::EMISSION_RAM_WORDS as usize;
const MOD_RAM_WORDS: usize = (fw::MOD_BUFFER_SAMPLES / 2) as usize;

const SELECT_CONTROLLER: usize = fw::BramSelect::Controller.as_u8() as usize;
const SELECT_PHASE_CORR: usize = fw::BramSelect::PhaseCorr.as_u8() as usize;
const SELECT_OUTPUT_MASK: usize = fw::BramSelect::OutputMask.as_u8() as usize;
const SELECT_FLASH: usize = fw::BramSelect::Flash.as_u8() as usize;
const SELECT_FLASH_BUF: usize = fw::BramSelect::FlashBuf.as_u8() as usize;
const SELECT_PWE_TABLE: usize = fw::BramSelect::PweTable.as_u8() as usize;
const SELECT_MOD: usize = fw::BramSelect::Mod.as_u8() as usize;
const SELECT_EMISSION: usize = fw::BramSelect::Emission.as_u8() as usize;

const LATCH_MASK: CtlFlags = CtlFlags::MOD_SET
    .union(CtlFlags::PATTERN_SET)
    .union(CtlFlags::SILENCER_SET)
    .union(CtlFlags::DEBUG_SET)
    .union(CtlFlags::SYNC_SET);

const MOD_CONFIG_REGS: [u16; 3] = [
    fw::ADDR_MOD_CYCLE0,
    fw::ADDR_MOD_FREQ_DIV0,
    fw::ADDR_MOD_REP0,
];
const PATTERN_CONFIG_REGS: [u16; 6] = [
    fw::ADDR_PATTERN_MODE0,
    fw::ADDR_PATTERN_CYCLE0,
    fw::ADDR_PATTERN_FREQ_DIV0,
    fw::ADDR_PATTERN_SOUND_SPEED0,
    fw::ADDR_PATTERN_NUM_FOCI0,
    fw::ADDR_PATTERN_REP0,
];
const SILENCER_REGS: core::ops::RangeInclusive<usize> =
    fw::ADDR_SILENCER_FLAG as usize..=fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE as usize;
const SILENCER_STRICT_MODE: u16 = fw::SilencerFlags::STRICT_MODE.bits() as u16;
const TRANSITION_MODE_EXT: u16 = fw::TransitionMode::Ext.as_u8() as u16;

struct SwapchainRegs {
    req_rd_bank: u16,
    transition_mode: u16,
    transition_value: u16,
    cycle: u16,
    freq_div: u16,
    rep: u16,
}

const MOD_SWAPCHAIN_REGS: SwapchainRegs = SwapchainRegs {
    req_rd_bank: fw::ADDR_MOD_REQ_RD_BANK,
    transition_mode: fw::ADDR_MOD_TRANSITION_MODE,
    transition_value: fw::ADDR_MOD_TRANSITION_VALUE_0,
    cycle: fw::ADDR_MOD_CYCLE0,
    freq_div: fw::ADDR_MOD_FREQ_DIV0,
    rep: fw::ADDR_MOD_REP0,
};
const PATTERN_SWAPCHAIN_REGS: SwapchainRegs = SwapchainRegs {
    req_rd_bank: fw::ADDR_PATTERN_REQ_RD_BANK,
    transition_mode: fw::ADDR_PATTERN_TRANSITION_MODE,
    transition_value: fw::ADDR_PATTERN_TRANSITION_VALUE_0,
    cycle: fw::ADDR_PATTERN_CYCLE0,
    freq_div: fw::ADDR_PATTERN_FREQ_DIV0,
    rep: fw::ADDR_PATTERN_REP0,
};

#[derive(Clone, Copy)]
enum Chain {
    Mod,
    Pattern,
}

impl Chain {
    const fn regs(self) -> &'static SwapchainRegs {
        match self {
            Self::Mod => &MOD_SWAPCHAIN_REGS,
            Self::Pattern => &PATTERN_SWAPCHAIN_REGS,
        }
    }

    const fn config_regs(self) -> &'static [u16] {
        match self {
            Self::Mod => &MOD_CONFIG_REGS,
            Self::Pattern => &PATTERN_CONFIG_REGS,
        }
    }

    const fn set_flag(self) -> CtlFlags {
        match self {
            Self::Mod => CtlFlags::MOD_SET,
            Self::Pattern => CtlFlags::PATTERN_SET,
        }
    }

    fn guard_steps(self, intensity: u16, phase: u16) -> u16 {
        match self {
            Self::Mod => intensity,
            Self::Pattern => intensity.max(phase),
        }
    }
}

struct BankRequest {
    req: usize,
    finite: bool,
    all: bool,
}

fn byte_at(words: &[u16], i: usize) -> u8 {
    (words[i >> 1] >> (8 * (i & 1))) as u8
}

const fn is_strict(silencer_flag: u16) -> bool {
    silencer_flag & SILENCER_STRICT_MODE != 0
        && silencer_flag & SILENCER_FIXED_UPDATE_RATE_MODE == 0
}
const CTL_FLAG_GPIO_IN: [CtlFlags; 4] = [
    CtlFlags::GPIO_IN_0,
    CtlFlags::GPIO_IN_1,
    CtlFlags::GPIO_IN_2,
    CtlFlags::GPIO_IN_3,
];
const SILENCER_FIXED_UPDATE_RATE_MODE: u16 =
    fw::SilencerFlags::FIXED_UPDATE_RATE_MODE.bits() as u16;

pub(crate) const fn reg(a: u16) -> usize {
    a as usize
}

pub struct FpgaEmulator {
    num_transducers: usize,
    pub(crate) controller: Box<[u16; 256]>,
    pub(crate) latched_config: Box<[u16; 256]>,
    phase_corr: Box<[u16; 256]>,
    output_mask: Box<[u16; OUTPUT_MASK_WORDS]>,
    pwe: Box<[u16; PWE_TABLE_SIZE]>,
    mod_ram: Vec<Box<[u16]>>,
    em_ram: Vec<Box<[u16]>>,
    pub(crate) next_sync_edge: u64,
    sys_time_ns: u64,
    pub(crate) thermal: bool,
    pub(crate) host_idle_ms: Option<u32>,
    #[cfg(feature = "udp")]
    pending_ptp_config: Option<autd3_cpu_fw::ptp::Config>,
    mod_swapchain: Swapchain,
    pattern_swapchain: Swapchain,
    cpu_flash: Box<[u8]>,
    reset_count: u32,
    pub(crate) flash: flash::FlashEmulator,
    reconfig_count: u32,
}

impl FpgaEmulator {
    #[must_use]
    pub(crate) fn new(num_transducers: usize) -> Self {
        let mut controller = Box::new([0u16; 256]);

        controller[reg(fw::ADDR_VERSION_NUM_MAJOR)] = fw::VERSION_NUM_MAJOR as u16;
        controller[reg(fw::ADDR_FUNCTION_BITS)] = u16::from(
            (FunctionBits::EMULATOR
                | FunctionBits::FLASH_OTA
                | FunctionBits::STRICT_SILENCER_GUARD)
                .bits(),
        );
        controller[reg(fw::ADDR_VERSION_NUM_MINOR)] = fw::VERSION_NUM_MINOR as u16;
        controller[reg(fw::ADDR_VERSION_NUM_PATCH)] = fw::VERSION_NUM_PATCH as u16;
        Self {
            num_transducers,
            controller,
            latched_config: Box::new([0u16; 256]),
            phase_corr: Box::new([0u16; 256]),
            output_mask: Box::new([0u16; OUTPUT_MASK_WORDS]),
            pwe: Box::new([0u16; PWE_TABLE_SIZE]),
            mod_ram: (0..NUM_BANKS)
                .map(|_| vec![0u16; MOD_RAM_WORDS].into_boxed_slice())
                .collect(),
            em_ram: (0..NUM_BANKS)
                .map(|_| vec![0u16; EMISSION_RAM_WORDS].into_boxed_slice())
                .collect(),
            next_sync_edge: 0,
            sys_time_ns: 0,
            thermal: false,
            host_idle_ms: None,
            #[cfg(feature = "udp")]
            pending_ptp_config: None,
            mod_swapchain: Swapchain::new(),
            pattern_swapchain: Swapchain::new(),
            cpu_flash: fresh_cpu_flash(),
            reset_count: 0,
            flash: flash::FlashEmulator::new(),
            reconfig_count: 0,
        }
    }

    pub(crate) fn power_on(&mut self) {
        self.reload();
    }

    fn reload(&mut self) {
        let mut next = Self::new(self.num_transducers);
        next.next_sync_edge = self.next_sync_edge;
        next.sys_time_ns = self.sys_time_ns;
        next.thermal = self.thermal;
        let old = core::mem::replace(self, next);
        self.cpu_flash = old.cpu_flash;
        self.reset_count = old.reset_count;
        self.flash = old.flash;
        self.flash.configure_from_flash();
        self.reconfig_count = old.reconfig_count;
    }

    #[must_use]
    pub fn fpga_flash(&self) -> &[u8] {
        self.flash.flash()
    }

    #[must_use]
    pub fn reconfig_count(&self) -> u32 {
        self.reconfig_count
    }

    #[must_use]
    pub fn cpu_flash(&self) -> &[u8] {
        &self.cpu_flash
    }

    pub fn cpu_flash_mut(&mut self) -> &mut [u8] {
        &mut self.cpu_flash
    }

    #[must_use]
    pub fn reset_count(&self) -> u32 {
        self.reset_count
    }

    pub(crate) fn note_reset(&mut self) {
        self.reset_count += 1;
    }

    #[cfg(feature = "udp")]
    pub(crate) const fn note_ptp_config(&mut self, config: autd3_cpu_fw::ptp::Config) {
        self.pending_ptp_config = Some(config);
    }

    #[cfg(feature = "udp")]
    pub(crate) const fn take_ptp_config(&mut self) -> Option<autd3_cpu_fw::ptp::Config> {
        self.pending_ptp_config.take()
    }

    pub(crate) fn write(&mut self, addr: u16, value: u16) {
        let select = (addr >> 8) as usize;
        let a = (addr & 0x3FFF) as usize;
        match select >> 6 {
            sel if sel == SELECT_MOD >> 6 => {
                let bank = self.controller[reg(fw::ADDR_MOD_MEM_WR_BANK)] as usize;
                let page = self.controller[reg(fw::ADDR_MOD_MEM_WR_PAGE)] as usize;
                self.mod_ram[bank][(page << 14) | a] = value;
            }
            sel if sel == SELECT_EMISSION >> 6 => {
                let bank = self.controller[reg(fw::ADDR_PATTERN_MEM_WR_BANK)] as usize;
                let page = self.controller[reg(fw::ADDR_PATTERN_MEM_WR_PAGE)] as usize;
                self.em_ram[bank][(page << 14) | a] = value;
            }
            _ => self.write_controller(addr as usize, value),
        }
    }

    fn write_controller(&mut self, a: usize, value: u16) {
        match a >> 8 {
            SELECT_CONTROLLER => {
                if a & 0xFF == reg(fw::ADDR_CTL_FLAG) {
                    let flags = CtlFlags::from_bits_retain(value);
                    self.controller[reg(fw::ADDR_CTL_FLAG)] = (flags & !LATCH_MASK).bits();
                    let mut rejected = CtlFlags::from_bits_retain(
                        self.controller[reg(fw::ADDR_SILENCER_SET_RESULT)],
                    ) & !flags;
                    for chain in [Chain::Mod, Chain::Pattern] {
                        if !flags.contains(chain.set_flag()) {
                            continue;
                        }
                        if self.swapchain_latch_rejected(chain) {
                            rejected |= chain.set_flag();
                        } else {
                            self.latch_config(chain.config_regs());
                            self.latch_swapchain_request(chain.regs());
                            self.arm_swapchain(chain);
                        }
                    }
                    if flags.contains(CtlFlags::SILENCER_SET) {
                        if self.silencer_latch_rejected() {
                            rejected |= CtlFlags::SILENCER_SET;
                        } else {
                            self.latched_config[SILENCER_REGS]
                                .copy_from_slice(&self.controller[SILENCER_REGS]);
                        }
                    }
                    self.controller[reg(fw::ADDR_SILENCER_SET_RESULT)] = rejected.bits();
                } else {
                    self.controller[a & 0xFF] = value;
                }
            }
            SELECT_PHASE_CORR => self.phase_corr[a & 0xFF] = value,
            SELECT_OUTPUT_MASK => self.output_mask[a & (OUTPUT_MASK_WORDS - 1)] = value,
            SELECT_PWE_TABLE => self.pwe[a & (PWE_TABLE_SIZE - 1)] = value,
            SELECT_FLASH => {
                self.flash.write_reg(a & 0xFF, value);
                if self.flash.take_reboot_request() {
                    self.reload();
                    self.reconfig_count += 1;
                }
            }
            sel if sel >> 1 == SELECT_FLASH_BUF >> 1 => self.flash.write_buf(a & 0x1FF, value),
            _ => {}
        }
    }

    pub(crate) fn read(&self, addr: u16) -> u16 {
        let select = (addr >> 8) as usize;
        let a = addr as usize;
        if select == SELECT_CONTROLLER {
            if a & 0xFF == reg(fw::ADDR_FPGA_STATE) {
                u16::from(self.fpga_state())
            } else {
                self.controller[a & 0xFF]
            }
        } else if select == SELECT_FLASH {
            self.flash.read_reg(a & 0xFF)
        } else {
            0
        }
    }

    pub(crate) fn next_sync_edge(&mut self) -> u64 {
        self.next_sync_edge.max(self.sys_time_ns + 500_000)
    }

    fn reg_u64(&self, base: usize) -> u64 {
        (0..4)
            .map(|i| u64::from(self.controller[base + i]) << (16 * i))
            .sum()
    }

    fn latch_config(&mut self, regs: &[u16]) {
        for base in regs {
            let range = reg(*base)..reg(*base) + NUM_BANKS;
            self.latched_config[range.clone()].copy_from_slice(&self.controller[range]);
        }
    }

    fn latch_swapchain_request(&mut self, regs: &SwapchainRegs) {
        for a in [reg(regs.req_rd_bank), reg(regs.transition_mode)] {
            self.latched_config[a] = self.controller[a];
        }
    }

    const fn swapchain(&self, chain: Chain) -> &Swapchain {
        match chain {
            Chain::Mod => &self.mod_swapchain,
            Chain::Pattern => &self.pattern_swapchain,
        }
    }

    const fn swapchain_mut(&mut self, chain: Chain) -> &mut Swapchain {
        match chain {
            Chain::Mod => &mut self.mod_swapchain,
            Chain::Pattern => &mut self.pattern_swapchain,
        }
    }

    fn latched_request(&self, chain: Chain) -> BankRequest {
        let regs = chain.regs();
        let req = self.latched_config[reg(regs.req_rd_bank)] as usize;
        let finite = self.latched_config[reg(regs.rep) + req] != fw::REP_INFINITE;
        let all = if finite {
            self.swapchain(chain).ext_active()
        } else {
            self.latched_config[reg(regs.transition_mode)] == TRANSITION_MODE_EXT
        };
        BankRequest { req, finite, all }
    }

    fn staged_request(&self, chain: Chain) -> BankRequest {
        let regs = chain.regs();
        let req = self.controller[reg(regs.req_rd_bank)] as usize;
        let finite = self.controller[reg(regs.rep) + req] != fw::REP_INFINITE;
        let all = if finite {
            self.latched_request(chain).all
        } else {
            self.controller[reg(regs.transition_mode)] == TRANSITION_MODE_EXT
        };
        BankRequest { req, finite, all }
    }

    fn div_below(
        &self,
        config: &[u16; 256],
        chain: Chain,
        request: &BankRequest,
        steps: u16,
    ) -> bool {
        let cur_bank = self.swapchain(chain).cur_bank();
        (0..NUM_BANKS).any(|bank| {
            (request.all || bank == request.req || (request.finite && bank == cur_bank))
                && config[reg(chain.regs().freq_div) + bank] < steps
        })
    }

    fn swapchain_latch_rejected(&self, chain: Chain) -> bool {
        is_strict(self.latched_config[reg(fw::ADDR_SILENCER_FLAG)])
            && self.div_below(
                &self.controller,
                chain,
                &self.staged_request(chain),
                chain.guard_steps(
                    self.silencer_completion_steps_intensity(),
                    self.silencer_completion_steps_phase(),
                ),
            )
    }

    fn silencer_latch_rejected(&self) -> bool {
        let intensity = self.controller[reg(fw::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY)];
        let phase = self.controller[reg(fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE)];
        is_strict(self.controller[reg(fw::ADDR_SILENCER_FLAG)])
            && [Chain::Mod, Chain::Pattern].into_iter().any(|chain| {
                self.div_below(
                    &self.latched_config,
                    chain,
                    &self.latched_request(chain),
                    chain.guard_steps(intensity, phase),
                )
            })
    }

    fn arm_swapchain(&mut self, chain: Chain) {
        let regs = chain.regs();
        let req = self.controller[reg(regs.req_rd_bank)] as usize;
        let rep = self.latched_config[reg(regs.rep) + req];
        let freq_div = self.latched_config[reg(regs.freq_div) + req];
        let cycle = self.latched_config[reg(regs.cycle) + req] as usize + 1;
        let mode = self.controller[reg(regs.transition_mode)] as u8;
        let value = self.reg_u64(reg(regs.transition_value));
        let sys_time_ns = self.sys_time_ns;
        self.swapchain_mut(chain)
            .set(sys_time_ns, rep, freq_div, cycle, req, mode, value);
    }

    fn effective_gpio_in(&self) -> [bool; 4] {
        let ctl = self.controller[reg(fw::ADDR_CTL_FLAG)];
        CTL_FLAG_GPIO_IN.map(|flag| CtlFlags::from_bits_retain(ctl).contains(flag))
    }

    pub fn update_with_sys_time(&mut self, sys_time_ns: u64) {
        self.sys_time_ns = sys_time_ns;
        let gpio_in = self.effective_gpio_in();
        self.mod_swapchain.update(gpio_in, sys_time_ns);
        self.pattern_swapchain.update(gpio_in, sys_time_ns);
    }

    #[must_use]
    pub fn force_fan(&self) -> bool {
        CtlFlags::from_bits_retain(self.controller[reg(fw::ADDR_CTL_FLAG)])
            .contains(CtlFlags::FORCE_FAN)
    }

    #[must_use]
    pub fn failsafe(&self) -> bool {
        CtlFlags::from_bits_retain(self.controller[reg(fw::ADDR_CTL_FLAG)])
            .contains(CtlFlags::FAILSAFE)
    }

    #[must_use]
    pub fn is_thermo_asserted(&self) -> bool {
        self.thermal
    }

    #[must_use]
    pub fn sys_time(&self) -> u64 {
        self.sys_time_ns
    }

    #[must_use]
    pub fn gpio_out(&self, i: usize) -> u64 {
        self.reg_u64(reg(fw::ADDR_DEBUG_VALUE0_0) + i * 4)
    }

    #[must_use]
    pub fn pulse_width_table(&self, key: usize) -> u16 {
        self.pwe[key & (PWE_TABLE_SIZE - 1)]
    }

    #[must_use]
    pub const fn host_idle_ms(&self) -> Option<u32> {
        self.host_idle_ms
    }

    #[must_use]
    pub fn fpga_state(&self) -> u8 {
        let mut state = FpgaStateFlags::empty();
        state.set(FpgaStateFlags::THERMAL_ASSERT, self.thermal);
        state.set(FpgaStateFlags::MOD_BANK, self.current_mod_bank() == 1);
        state.set(
            FpgaStateFlags::PATTERN_BANK,
            self.current_pattern_bank() == 1,
        );
        state.set(FpgaStateFlags::PATTERN_MODE, self.is_pattern_mode());
        state.set(
            FpgaStateFlags::PATTERN_STOPPED,
            self.pattern_swapchain.stopped(),
        );
        state.set(FpgaStateFlags::MOD_STOPPED, self.mod_swapchain.stopped());
        state.set(
            FpgaStateFlags::TRANSITION_PENDING,
            self.pattern_swapchain.transition_pending() || self.mod_swapchain.transition_pending(),
        );
        state.set(FpgaStateFlags::FAILSAFE, self.failsafe());
        state.bits()
    }

    #[must_use]
    pub fn is_pattern_mode(&self) -> bool {
        self.pattern_cycle(self.current_pattern_bank()) == 1
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
    pub fn emissions(&self) -> (Vec<Phase>, Vec<Intensity>) {
        let bank = self.current_pattern_bank();
        let idx = self.current_pattern_idx();
        if self.pattern_mode(bank) == u16::from(fw::EmissionType::Foci.as_u8()) {
            self.foci_emissions_at(bank, idx)
        } else {
            self.emissions_at(bank, idx)
        }
    }

    #[must_use]
    pub fn modulation(&self) -> u8 {
        self.modulation_at(self.current_mod_bank(), self.current_mod_idx())
    }

    #[must_use]
    pub fn silencer_fixed_update_rate_mode(&self) -> bool {
        self.latched_config[reg(fw::ADDR_SILENCER_FLAG)] & SILENCER_FIXED_UPDATE_RATE_MODE != 0
    }

    #[must_use]
    pub fn silencer_update_rate_intensity(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_SILENCER_UPDATE_RATE_INTENSITY)]
    }

    #[must_use]
    pub fn silencer_update_rate_phase(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_SILENCER_UPDATE_RATE_PHASE)]
    }

    #[must_use]
    pub fn silencer_completion_steps_intensity(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY)]
    }

    #[must_use]
    pub fn silencer_completion_steps_phase(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE)]
    }

    fn silencer_emulator(&self, is_phase: bool, initial: u8) -> SilencerEmulator {
        let fixed_rate = self.silencer_fixed_update_rate_mode();
        let value = match (fixed_rate, is_phase) {
            (true, true) => self.silencer_update_rate_phase(),
            (true, false) => self.silencer_update_rate_intensity(),
            (false, true) => self.silencer_completion_steps_phase(),
            (false, false) => self.silencer_completion_steps_intensity(),
        };
        SilencerEmulator::new(is_phase, initial, fixed_rate, value)
    }

    #[must_use]
    pub fn silencer_emulator_phase(&self, initial: u8) -> SilencerEmulator {
        self.silencer_emulator(true, initial)
    }

    #[must_use]
    pub fn silencer_emulator_intensity(&self, initial: u8) -> SilencerEmulator {
        self.silencer_emulator(false, initial)
    }

    #[must_use]
    pub fn num_transducers(&self) -> usize {
        self.num_transducers
    }

    #[must_use]
    pub fn fpga_version(&self) -> (u16, u16, u16) {
        (
            self.controller[reg(fw::ADDR_VERSION_NUM_MAJOR)],
            self.controller[reg(fw::ADDR_VERSION_NUM_MINOR)],
            self.controller[reg(fw::ADDR_VERSION_NUM_PATCH)],
        )
    }

    #[must_use]
    pub fn modulation_cycle(&self, bank: usize) -> usize {
        self.latched_config[reg(fw::ADDR_MOD_CYCLE0) + bank] as usize + 1
    }

    #[must_use]
    pub fn modulation_freq_div(&self, bank: usize) -> u16 {
        self.latched_config[reg(fw::ADDR_MOD_FREQ_DIV0) + bank]
    }

    #[must_use]
    pub fn pattern_cycle(&self, bank: usize) -> usize {
        self.latched_config[reg(fw::ADDR_PATTERN_CYCLE0) + bank] as usize + 1
    }

    #[must_use]
    pub fn pattern_freq_div(&self, bank: usize) -> u16 {
        self.latched_config[reg(fw::ADDR_PATTERN_FREQ_DIV0) + bank]
    }

    #[must_use]
    pub fn pattern_mode(&self, bank: usize) -> u16 {
        self.latched_config[reg(fw::ADDR_PATTERN_MODE0) + bank]
    }

    #[must_use]
    pub fn modulation_at(&self, bank: usize, idx: usize) -> u8 {
        byte_at(&self.mod_ram[bank], idx)
    }

    #[must_use]
    pub fn modulation_buffer(&self, bank: usize) -> Vec<u8> {
        (0..self.modulation_cycle(bank))
            .map(|i| self.modulation_at(bank, i))
            .collect()
    }

    #[must_use]
    pub fn phase_correction(&self, i: usize) -> Phase {
        Phase(byte_at(self.phase_corr.as_slice(), i))
    }

    #[must_use]
    pub fn output_mask_enabled(&self, i: usize) -> bool {
        self.output_mask[i >> 4] & (1 << (i & 0x0F)) != 0
    }

    fn emits(&self, i: usize) -> bool {
        self.output_mask_enabled(i) && !self.failsafe()
    }

    #[must_use]
    pub fn to_pulse_width(&self, intensity: Intensity, modulation: u8) -> u16 {
        let key = (intensity.0 as usize * modulation as usize) / 255;
        self.pwe[key & (PWE_TABLE_SIZE - 1)]
    }

    #[must_use]
    pub fn emissions_at(&self, bank: usize, idx: usize) -> (Vec<Phase>, Vec<Intensity>) {
        let base = idx * EMISSION_SLOT_WORDS;
        (0..self.num_transducers)
            .map(|i| {
                let word = self.em_ram[bank][base + i];
                let phase = Phase((word & 0xFF) as u8) + self.phase_correction(i);
                let intensity = if self.emits(i) {
                    Intensity((word >> 8) as u8)
                } else {
                    Intensity::MIN
                };
                (phase, intensity)
            })
            .unzip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(fpga: &mut FpgaEmulator, addr: u16, value: u16) {
        fpga.write(addr, value);
    }

    #[test]
    fn functions_byte_has_emulator_bit() {
        let fpga = FpgaEmulator::new(249);
        let functions = fpga.read(fw::ADDR_FUNCTION_BITS) as u8;
        assert!(FunctionBits::from_bits_retain(functions).contains(FunctionBits::EMULATOR));
    }

    #[test]
    fn silencer_fixed_update_rate_intensity() {
        let mut fpga = FpgaEmulator::new(249);
        write(
            &mut fpga,
            fw::ADDR_SILENCER_FLAG,
            SILENCER_FIXED_UPDATE_RATE_MODE,
        );
        write(&mut fpga, fw::ADDR_SILENCER_UPDATE_RATE_INTENSITY, 1);
        write(&mut fpga, fw::ADDR_CTL_FLAG, CtlFlags::SILENCER_SET.bits());
        let mut s = fpga.silencer_emulator_intensity(0);
        let out: Vec<u8> = (0..256).map(|_| s.apply(1)).collect();
        let mut expect = vec![0u8; 255];
        expect.push(1);
        assert_eq!(expect, out);
    }

    #[test]
    fn silencer_completion_steps_intensity() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, fw::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, 10);
        write(&mut fpga, fw::ADDR_CTL_FLAG, CtlFlags::SILENCER_SET.bits());
        let mut s = fpga.silencer_emulator_intensity(10);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(vec![21, 33, 45, 57, 69, 80, 92, 104, 116, 128, 128], out);
    }

    #[test]
    fn silencer_completion_steps_phase() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE, 10);
        write(&mut fpga, fw::ADDR_CTL_FLAG, CtlFlags::SILENCER_SET.bits());
        let mut s = fpga.silencer_emulator_phase(0);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(vec![12, 25, 38, 51, 64, 76, 89, 102, 115, 128, 128], out);
    }

    #[test]
    fn silencer_phase_wraps_shortest_path() {
        let mut fpga = FpgaEmulator::new(249);
        write(&mut fpga, fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE, 10);
        write(&mut fpga, fw::ADDR_CTL_FLAG, CtlFlags::SILENCER_SET.bits());
        let mut s = fpga.silencer_emulator_phase(180);
        let out: Vec<u8> = (0..11).map(|_| s.apply(128)).collect();
        assert_eq!(
            vec![174, 169, 164, 159, 153, 148, 143, 138, 133, 128, 128],
            out
        );
    }

    fn latch_mod(
        fpga: &mut FpgaEmulator,
        req: u16,
        mode: u8,
        div: [u16; 2],
        rep: [u16; 2],
    ) -> bool {
        write(fpga, fw::ADDR_MOD_REQ_RD_BANK, req);
        write(fpga, fw::ADDR_MOD_TRANSITION_MODE, u16::from(mode));
        for bank in 0..2 {
            write(fpga, fw::ADDR_MOD_CYCLE0 + bank, 1);
            write(fpga, fw::ADDR_MOD_FREQ_DIV0 + bank, div[usize::from(bank)]);
            write(fpga, fw::ADDR_MOD_REP0 + bank, rep[usize::from(bank)]);
        }
        write(fpga, fw::ADDR_CTL_FLAG, CtlFlags::MOD_SET.bits());
        fpga.read(fw::ADDR_SILENCER_SET_RESULT) & CtlFlags::MOD_SET.bits() == 0
    }

    fn latch_silencer(fpga: &mut FpgaEmulator, flag: u16, intensity: u16, phase: u16) -> bool {
        write(fpga, fw::ADDR_SILENCER_FLAG, flag);
        write(
            fpga,
            fw::ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
            intensity,
        );
        write(fpga, fw::ADDR_SILENCER_COMPLETION_STEPS_PHASE, phase);
        write(fpga, fw::ADDR_CTL_FLAG, CtlFlags::SILENCER_SET.bits());
        fpga.read(fw::ADDR_SILENCER_SET_RESULT) & CtlFlags::SILENCER_SET.bits() == 0
    }

    fn guarded_fpga() -> FpgaEmulator {
        let mut fpga = FpgaEmulator::new(249);
        for bank in 0..2 {
            write(&mut fpga, fw::ADDR_PATTERN_FREQ_DIV0 + bank, 0xFFFF);
            write(&mut fpga, fw::ADDR_PATTERN_REP0 + bank, fw::REP_INFINITE);
        }
        write(&mut fpga, fw::ADDR_CTL_FLAG, CtlFlags::PATTERN_SET.bits());
        fpga
    }

    const INF: u16 = fw::REP_INFINITE;
    const IMMEDIATE: u8 = fw::TransitionMode::Immediate.as_u8();

    #[test]
    fn functions_byte_has_strict_silencer_guard_bit() {
        let fpga = FpgaEmulator::new(249);
        let functions = fpga.read(fw::ADDR_FUNCTION_BITS) as u8;
        assert!(
            FunctionBits::from_bits_retain(functions).contains(FunctionBits::STRICT_SILENCER_GUARD)
        );
    }

    #[test]
    fn strict_is_accepted_right_after_an_immediate_switch_off_a_faster_bank() {
        let mut fpga = guarded_fpga();
        assert!(latch_mod(&mut fpga, 0, IMMEDIATE, [5, 100], [INF, INF]));
        assert!(latch_mod(&mut fpga, 1, IMMEDIATE, [5, 100], [INF, INF]));
        fpga.mod_swapchain.force_cur_bank(0);
        assert!(latch_silencer(&mut fpga, SILENCER_STRICT_MODE, 8, 8));
    }
}
