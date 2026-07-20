pub const ULTRASOUND_FREQ_HZ: u32 = 40_000;

pub const NUM_BANKS: usize = autd3_cpu_wire::params::NUM_BANKS;

pub const MOD_BUFFER_SAMPLES: usize = autd3_cpu_wire::params::MOD_BUFFER_SAMPLES;

pub const EMISSION_SLOT_WORDS: usize = 256;
pub const EMISSION_MAX_INDICES: usize = autd3_cpu_wire::params::EMISSION_MAX_INDICES as usize;
pub const EMISSION_RAM_WORDS: usize = EMISSION_SLOT_WORDS * EMISSION_MAX_INDICES;

pub const FOCUS_WORDS: usize = 4;

pub const MAX_FOCI_TOTAL: usize = EMISSION_RAM_WORDS / FOCUS_WORDS;

pub const NUM_FOCI_MAX: u8 = autd3_cpu_wire::params::NUM_FOCI_MAX;

// 18-bit signed coordinate, in 0.025 mm units.
pub const FOCUS_COORD_MIN: i32 = -131_072;
pub const FOCUS_COORD_MAX: i32 = 131_071;

pub const FOCUS_TR_X_MAX: i32 = 0x1AFC;
pub const FOCUS_TR_Y_MAX: i32 = 0x14A3;

pub const FOCUS_COORD_MIN_X: i32 = FOCUS_COORD_MIN + FOCUS_TR_X_MAX;
pub const FOCUS_COORD_MAX_X: i32 = FOCUS_COORD_MAX;
pub const FOCUS_COORD_MIN_Y: i32 = FOCUS_COORD_MIN + FOCUS_TR_Y_MAX;
pub const FOCUS_COORD_MAX_Y: i32 = FOCUS_COORD_MAX;
pub const FOCUS_COORD_MIN_Z: i32 = FOCUS_COORD_MIN;
pub const FOCUS_COORD_MAX_Z: i32 = FOCUS_COORD_MAX;
