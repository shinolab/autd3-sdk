pub const ERR_INVALID_SILENCER_SETTING: u8 = 0x04;
pub const ERR_INVALID_TRANSITION_MODE: u8 = 0x05;
pub const ERR_MISS_TRANSITION_TIME: u8 = 0x06;

pub mod error;
pub mod foci_stm;
pub mod force_fan;
pub mod gpio;
pub mod modulation;
pub mod output_mask;
pub mod pattern;
pub mod pattern_stm;
pub mod pattern_util;
pub mod phase_correction;
pub mod pulse_width_encoder;
pub mod silencer;
pub mod transition;
