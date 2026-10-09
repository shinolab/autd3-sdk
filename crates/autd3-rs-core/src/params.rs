pub use autd3_cpu_wire::fpga_params::{FPGA_CLK_FREQ_HZ, REP_INFINITE, ULTRASOUND_FREQ_HZ};

pub const NUM_BANKS: usize = autd3_cpu_wire::fpga_params::NUM_BANKS;

pub const MOD_BUFFER_SAMPLES: usize = autd3_cpu_wire::fpga_params::MOD_BUFFER_SAMPLES;

pub use autd3_cpu_wire::layout::{BUFFER_SIZE_MIN, FOCUS_WORDS, MAX_FOCI_TOTAL};

pub const EMISSION_MAX_INDICES: usize = autd3_cpu_wire::fpga_params::EMISSION_MAX_INDICES as usize;

pub const NUM_FOCI_MAX: u8 = autd3_cpu_wire::fpga_params::NUM_FOCI_MAX;

pub use autd3_cpu_wire::layout::{FOCUS_COORD_MAX, FOCUS_COORD_MIN};

pub use autd3_cpu_wire::fpga_params::{FOCUS_TR_X_MAX, FOCUS_TR_Y_MAX};

pub use autd3_cpu_wire::fpga_params::SilencerFlags;
pub use autd3_cpu_wire::payload::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE,
};
