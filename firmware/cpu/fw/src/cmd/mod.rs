pub(crate) mod change_mod_bank;
pub(crate) mod change_pattern_bank;
pub(crate) mod clear;
pub(crate) mod config_mod;
pub(crate) mod config_pattern;
pub(crate) mod failsafe;
pub(crate) mod force_fan;
pub(crate) mod fpga_update;
pub(crate) mod gpio_in;
pub(crate) mod gpio_out;
pub(crate) mod output_mask;
pub(crate) mod phase_corr;
pub(crate) mod pwe;
pub(crate) mod set_mode;
pub(crate) mod silencer;
pub(crate) mod sync;
pub(crate) mod update;
pub(crate) mod write_foci;
pub(crate) mod write_mod;
pub(crate) mod write_mod_fused;
pub(crate) mod write_pattern_compressed;
pub(crate) mod write_pattern_fused;
pub(crate) mod write_pattern_raw;

use crate::fpga::{SYS_TIME_TRANSITION_MARGIN_NS, TransitionMode};

pub(crate) struct TransitionRequest {
    pub(crate) mode: TransitionMode,
    pub(crate) value: u64,
    pub(crate) margin_ns: u32,
}

impl TransitionRequest {
    pub(crate) fn margin_ns(&self) -> u64 {
        if self.margin_ns == 0 {
            SYS_TIME_TRANSITION_MARGIN_NS
        } else {
            u64::from(self.margin_ns)
        }
    }
}

pub(crate) struct BankChange {
    pub(crate) bank: u8,
    pub(crate) transition_mode: TransitionMode,
    pub(crate) transition_value: u64,
}
