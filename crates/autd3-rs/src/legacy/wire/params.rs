pub const CPU_VERSION_V12_1: u8 = 0xA5;

pub const MOD_BUF_SIZE_MIN: usize = 2;
pub const MOD_BUF_SIZE_MAX: usize = 65536;
pub const MOD_HEAD_SIZE_MAX: usize = 254;

pub const STM_BUF_SIZE_MIN: usize = 2;
pub const FOCI_STM_BUF_SIZE_MAX: usize = 65536;
pub const GAIN_STM_BUF_SIZE_MAX: usize = 1024;

pub const FOCI_STM_FOCI_NUM_MIN: usize = 1;
pub const FOCI_STM_FOCI_NUM_MAX: usize = 8;

pub const REP_INFINITE: u16 = 0xFFFF;

pub const TRANSITION_MODE_SYNC_IDX: u8 = 0x00;
pub const TRANSITION_MODE_SYS_TIME: u8 = 0x01;
pub const TRANSITION_MODE_GPIO: u8 = 0x02;
pub const TRANSITION_MODE_EXT: u8 = 0xF0;
pub const TRANSITION_MODE_NONE: u8 = 0xFE;
pub const TRANSITION_MODE_IMMEDIATE: u8 = 0xFF;

pub const GAIN_FLAG_UPDATE: u8 = 1 << 0;

pub const MODULATION_FLAG_BEGIN: u8 = 1 << 0;
pub const MODULATION_FLAG_END: u8 = 1 << 1;
pub const MODULATION_FLAG_TRANSITION: u8 = 1 << 2;
pub const MODULATION_FLAG_SEGMENT: u8 = 1 << 3;

pub const FOCI_STM_FLAG_BEGIN: u8 = 1 << 0;
pub const FOCI_STM_FLAG_END: u8 = 1 << 1;
pub const FOCI_STM_FLAG_TRANSITION: u8 = 1 << 2;

pub const GAIN_STM_FLAG_BEGIN: u8 = 1 << 0;
pub const GAIN_STM_FLAG_END: u8 = 1 << 1;
pub const GAIN_STM_FLAG_TRANSITION: u8 = 1 << 2;
pub const GAIN_STM_FLAG_SEGMENT: u8 = 1 << 3;
pub const GAIN_STM_FLAG_SEND_BIT0: u8 = 1 << 6;
pub const GAIN_STM_FLAG_SEND_BIT1: u8 = 1 << 7;

pub const SILENCER_FLAG_FIXED_UPDATE_RATE_MODE: u8 = 1 << 0;
pub const SILENCER_FLAG_STRICT_MODE: u8 = 1 << 2;

pub const SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY: u16 = 10;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_PHASE: u16 = 40;
pub const SILENCER_DEFAULT_UPDATE_RATE: u16 = 256;

pub const SYS_TIME_TRANSITION_MARGIN_NS: u64 = 10_000_000;

pub const PWE_TABLE_SIZE: usize = 256;

pub const GPIO_IN_FLAG_0: u8 = 1 << 0;
pub const GPIO_IN_FLAG_1: u8 = 1 << 1;
pub const GPIO_IN_FLAG_2: u8 = 1 << 2;
pub const GPIO_IN_FLAG_3: u8 = 1 << 3;

pub const GPIO_O_TYPE_NONE: u8 = 0x00;
pub const GPIO_O_TYPE_BASE_SIG: u8 = 0x01;
pub const GPIO_O_TYPE_THERMO: u8 = 0x02;
pub const GPIO_O_TYPE_FORCE_FAN: u8 = 0x03;
pub const GPIO_O_TYPE_SYNC: u8 = 0x10;
pub const GPIO_O_TYPE_MOD_SEGMENT: u8 = 0x20;
pub const GPIO_O_TYPE_MOD_IDX: u8 = 0x21;
pub const GPIO_O_TYPE_STM_SEGMENT: u8 = 0x50;
pub const GPIO_O_TYPE_STM_IDX: u8 = 0x51;
pub const GPIO_O_TYPE_IS_STM_MODE: u8 = 0x52;
pub const GPIO_O_TYPE_SYS_TIME_EQ: u8 = 0x60;
pub const GPIO_O_TYPE_SYNC_DIFF: u8 = 0x70;
pub const GPIO_O_TYPE_PWM_OUT: u8 = 0xE0;
pub const GPIO_O_TYPE_DIRECT: u8 = 0xF0;

pub const GPIO_O_VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;
