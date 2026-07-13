use core::cell::Cell;

use zerocopy::FromBytes;

use crate::app::Cpu;
use crate::fpga;
use crate::fpga::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
};
use crate::params::{
    ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
    ADDR_SILENCER_FLAG, ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE,
    BRAM_SELECT_CONTROLLER, CTL_FLAG_SILENCER_SET, NUM_BANKS, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
};
use crate::port::Port;
use crate::proto::{
    ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING, SILENCER_FLAG_STRICT_MODE, SilencerPayload,
};

pub(crate) struct SilencerGuard {
    strict_mode: Cell<bool>,
    completion_intensity: Cell<u16>,
    completion_phase: Cell<u16>,
    mod_freq_div: [Cell<u16>; NUM_BANKS],
    pattern_freq_div: [Cell<u16>; NUM_BANKS],
    mod_bank: Cell<u8>,
    pattern_bank: Cell<u8>,
}

impl SilencerGuard {
    pub(crate) const fn new() -> Self {
        Self {
            strict_mode: Cell::new(false),
            completion_intensity: Cell::new(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY),
            completion_phase: Cell::new(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE),
            mod_freq_div: [const { Cell::new(0xFFFF) }; NUM_BANKS],
            pattern_freq_div: [const { Cell::new(0xFFFF) }; NUM_BANKS],
            mod_bank: Cell::new(0),
            pattern_bank: Cell::new(0),
        }
    }

    pub(crate) fn init(&self) {
        self.strict_mode.set(false);
        self.completion_intensity
            .set(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY);
        self.completion_phase
            .set(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE);
        for div in &self.mod_freq_div {
            div.set(0xFFFF);
        }
        for div in &self.pattern_freq_div {
            div.set(0xFFFF);
        }
        self.mod_bank.set(0);
        self.pattern_bank.set(0);
    }

    pub(crate) fn violates_mod_div(&self, divider: u16) -> bool {
        self.strict_mode.get() && divider < self.completion_intensity.get()
    }

    pub(crate) fn violates_pattern_div(&self, divider: u16) -> bool {
        self.strict_mode.get()
            && (divider < self.completion_intensity.get() || divider < self.completion_phase.get())
    }

    pub(crate) fn violates_mod_bank(&self, bank: u8) -> bool {
        self.violates_mod_div(self.mod_freq_div[bank as usize].get())
    }

    pub(crate) fn violates_pattern_bank(&self, bank: u8) -> bool {
        self.violates_pattern_div(self.pattern_freq_div[bank as usize].get())
    }

    pub(crate) fn note_mod_div(&self, bank: u8, divider: u16) {
        self.mod_freq_div[bank as usize].set(divider);
    }

    pub(crate) fn note_pattern_div(&self, bank: u8, divider: u16) {
        self.pattern_freq_div[bank as usize].set(divider);
    }

    pub(crate) fn note_mod_bank(&self, bank: u8) {
        self.mod_bank.set(bank);
    }

    pub(crate) fn note_pattern_bank(&self, bank: u8) {
        self.pattern_bank.set(bank);
    }
}

impl Cpu {
    pub(crate) fn set_silencer<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let Ok((p, _)) = SilencerPayload::ref_from_prefix(payload) else {
            return ERR_INVALID_PAYLOAD;
        };
        let flag = p.flag;
        let update_rate_intensity = p.update_rate_intensity.get();
        let update_rate_phase = p.update_rate_phase.get();
        let completion_steps_intensity = p.completion_steps_intensity.get();
        let completion_steps_phase = p.completion_steps_phase.get();

        if (flag & SILENCER_FLAG_FIXED_UPDATE_RATE_MODE) != 0 {
            if update_rate_intensity == 0 || update_rate_phase == 0 {
                return ERR_INVALID_PAYLOAD;
            }
            self.silencer.strict_mode.set(false);
        } else {
            if completion_steps_intensity == 0 || completion_steps_phase == 0 {
                return ERR_INVALID_PAYLOAD;
            }
            if (flag & SILENCER_FLAG_STRICT_MODE) != 0 {
                let mod_div =
                    self.silencer.mod_freq_div[self.silencer.mod_bank.get() as usize].get();
                let pattern_div =
                    self.silencer.pattern_freq_div[self.silencer.pattern_bank.get() as usize].get();
                if mod_div < completion_steps_intensity
                    || pattern_div < completion_steps_intensity
                    || pattern_div < completion_steps_phase
                {
                    return ERR_INVALID_SILENCER_SETTING;
                }
                self.silencer.strict_mode.set(true);
            } else {
                self.silencer.strict_mode.set(false);
            }
            self.silencer
                .completion_intensity
                .set(completion_steps_intensity);
            self.silencer.completion_phase.set(completion_steps_phase);
        }

        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SILENCER_UPDATE_RATE_INTENSITY,
            update_rate_intensity,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SILENCER_UPDATE_RATE_PHASE,
            update_rate_phase,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
            completion_steps_intensity,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SILENCER_COMPLETION_STEPS_PHASE,
            completion_steps_phase,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SILENCER_FLAG,
            u16::from(flag),
        );
        self.set_and_wait_update(port, CTL_FLAG_SILENCER_SET)
    }
}
