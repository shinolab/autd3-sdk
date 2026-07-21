pub use autd3_cpu_wire::params::{FPGA_CLK_FREQ_HZ, REP_INFINITE, ULTRASOUND_FREQ_HZ};

pub const NUM_BANKS: usize = autd3_cpu_wire::params::NUM_BANKS;

pub const MOD_BUFFER_SAMPLES: usize = autd3_cpu_wire::params::MOD_BUFFER_SAMPLES;

pub use autd3_cpu_wire::layout::{
    EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, FOCUS_WORDS, MAX_FOCI_TOTAL,
};

pub const EMISSION_MAX_INDICES: usize = autd3_cpu_wire::params::EMISSION_MAX_INDICES as usize;

pub const NUM_FOCI_MAX: u8 = autd3_cpu_wire::params::NUM_FOCI_MAX;

// 18-bit signed coordinate, in 0.025 mm units.
pub const FOCUS_COORD_MIN: i32 = -(1 << 17);
pub const FOCUS_COORD_MAX: i32 = (1 << 17) - 1;

pub use autd3_cpu_wire::params::{FOCUS_TR_X_MAX, FOCUS_TR_Y_MAX};

pub const FOCUS_COORD_MIN_X: i32 = FOCUS_COORD_MIN + FOCUS_TR_X_MAX;
pub const FOCUS_COORD_MAX_X: i32 = FOCUS_COORD_MAX;
pub const FOCUS_COORD_MIN_Y: i32 = FOCUS_COORD_MIN + FOCUS_TR_Y_MAX;
pub const FOCUS_COORD_MAX_Y: i32 = FOCUS_COORD_MAX;
pub const FOCUS_COORD_MIN_Z: i32 = FOCUS_COORD_MIN;
pub const FOCUS_COORD_MAX_Z: i32 = FOCUS_COORD_MAX;
