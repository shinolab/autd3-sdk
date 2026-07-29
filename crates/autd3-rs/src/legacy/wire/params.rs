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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_buffer_limits_match_legacy_firmware() {
        assert_eq!(CPU_VERSION_V12_1, 0xA5);

        assert_eq!(MOD_BUF_SIZE_MIN, 2);
        assert_eq!(MOD_BUF_SIZE_MAX, 65536);
        assert_eq!(MOD_HEAD_SIZE_MAX, 254);

        assert_eq!(STM_BUF_SIZE_MIN, 2);
        assert_eq!(FOCI_STM_BUF_SIZE_MAX, 65536);
        assert_eq!(GAIN_STM_BUF_SIZE_MAX, 1024);

        assert_eq!(FOCI_STM_FOCI_NUM_MIN, 1);
        assert_eq!(FOCI_STM_FOCI_NUM_MAX, 8);

        assert_eq!(PWE_TABLE_SIZE, 256);
    }

    #[test]
    fn transition_modes_match_legacy_firmware() {
        assert_eq!(REP_INFINITE, 0xFFFF);
        assert_eq!(TRANSITION_MODE_SYNC_IDX, 0x00);
        assert_eq!(TRANSITION_MODE_SYS_TIME, 0x01);
        assert_eq!(TRANSITION_MODE_GPIO, 0x02);
        assert_eq!(TRANSITION_MODE_EXT, 0xF0);
        assert_eq!(TRANSITION_MODE_NONE, 0xFE);
        assert_eq!(TRANSITION_MODE_IMMEDIATE, 0xFF);
        assert_eq!(SYS_TIME_TRANSITION_MARGIN_NS, 10_000_000);
    }

    #[test]
    fn operation_flags_match_legacy_firmware() {
        assert_eq!(GAIN_FLAG_UPDATE, 0x01);

        assert_eq!(MODULATION_FLAG_BEGIN, 0x01);
        assert_eq!(MODULATION_FLAG_END, 0x02);
        assert_eq!(MODULATION_FLAG_TRANSITION, 0x04);
        assert_eq!(MODULATION_FLAG_SEGMENT, 0x08);

        assert_eq!(FOCI_STM_FLAG_BEGIN, 0x01);
        assert_eq!(FOCI_STM_FLAG_END, 0x02);
        assert_eq!(FOCI_STM_FLAG_TRANSITION, 0x04);

        assert_eq!(GAIN_STM_FLAG_BEGIN, 0x01);
        assert_eq!(GAIN_STM_FLAG_END, 0x02);
        assert_eq!(GAIN_STM_FLAG_TRANSITION, 0x04);
        assert_eq!(GAIN_STM_FLAG_SEGMENT, 0x08);
        assert_eq!(GAIN_STM_FLAG_SEND_BIT0, 0x40);
        assert_eq!(GAIN_STM_FLAG_SEND_BIT1, 0x80);

        assert_eq!(SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, 0x01);
        assert_eq!(SILENCER_FLAG_STRICT_MODE, 0x04);

        assert_eq!(GPIO_IN_FLAG_0, 0x01);
        assert_eq!(GPIO_IN_FLAG_1, 0x02);
        assert_eq!(GPIO_IN_FLAG_2, 0x04);
        assert_eq!(GPIO_IN_FLAG_3, 0x08);
    }

    #[test]
    fn silencer_defaults_match_the_firmware_boot_state() {
        assert_eq!(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, 10);
        assert_eq!(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE, 40);
        assert_eq!(SILENCER_DEFAULT_UPDATE_RATE, 256);
    }

    #[test]
    fn gpio_out_types_match_legacy_firmware() {
        assert_eq!(GPIO_O_TYPE_NONE, 0x00);
        assert_eq!(GPIO_O_TYPE_BASE_SIG, 0x01);
        assert_eq!(GPIO_O_TYPE_THERMO, 0x02);
        assert_eq!(GPIO_O_TYPE_FORCE_FAN, 0x03);
        assert_eq!(GPIO_O_TYPE_SYNC, 0x10);
        assert_eq!(GPIO_O_TYPE_MOD_SEGMENT, 0x20);
        assert_eq!(GPIO_O_TYPE_MOD_IDX, 0x21);
        assert_eq!(GPIO_O_TYPE_STM_SEGMENT, 0x50);
        assert_eq!(GPIO_O_TYPE_STM_IDX, 0x51);
        assert_eq!(GPIO_O_TYPE_IS_STM_MODE, 0x52);
        assert_eq!(GPIO_O_TYPE_SYS_TIME_EQ, 0x60);
        assert_eq!(GPIO_O_TYPE_SYNC_DIFF, 0x70);
        assert_eq!(GPIO_O_TYPE_PWM_OUT, 0xE0);
        assert_eq!(GPIO_O_TYPE_DIRECT, 0xF0);
        assert_eq!(GPIO_O_VALUE_MASK, 0x00FF_FFFF_FFFF_FFFF);
    }
}
