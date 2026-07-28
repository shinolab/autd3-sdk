pub const TYPE: u16 = 0x0000;
pub const DL_STATUS: u16 = 0x0110;
pub const DL_STATUS_PORT0_LINK: u16 = 1 << 4;
pub const DL_STATUS_PORT1_LINK: u16 = 1 << 5;
pub const AL_CONTROL: u16 = 0x0120;
pub const AL_STATUS: u16 = 0x0130;
pub const AL_STATUS_CODE: u16 = 0x0134;

pub const DL_CONTROL: u16 = 0x0100;
pub const STATION_ADDRESS: u16 = 0x0010;

pub const EEPROM_CONFIGURATION: u16 = 0x0500;
pub const SII_CONTROL: u16 = 0x0502;
pub const SII_ADDRESS: u16 = 0x0504;
pub const SII_DATA: u16 = 0x0508;

pub const SII_WORD_VENDOR_ID: u16 = 0x0008;
pub const SII_WORD_PRODUCT_CODE: u16 = 0x000a;
pub const SII_WORD_REVISION: u16 = 0x000c;
pub const SII_WORD_SERIAL: u16 = 0x000e;

pub const FMMU0: u16 = 0x0600;
pub const FMMU_STRIDE: u16 = 0x0010;

pub const SM0: u16 = 0x0800;
pub const SM_STRIDE: u16 = 0x0008;

pub const WATCHDOG_DIVIDER: u16 = 0x0400;
pub const WATCHDOG_TIME_PDI: u16 = 0x0410;
pub const WATCHDOG_TIME_PROCESS_DATA: u16 = 0x0420;

pub const DC_RECEIVE_TIME_PORT0: u16 = 0x0900;
pub const DC_SYSTEM_TIME: u16 = 0x0910;
pub const DC_SYSTEM_TIME_OFFSET: u16 = 0x0920;
pub const DC_SYSTEM_TIME_DELAY: u16 = 0x0928;
pub const DC_SPEED_COUNTER_START: u16 = 0x0930;
pub const DC_SYSTEM_TIME_DIFFERENCE: u16 = 0x092c;
pub const DC_SYNC_ACTIVATION: u16 = 0x0981;
pub const DC_SYNC_START_TIME: u16 = 0x0990;
pub const DC_SYNC0_CYCLE_TIME: u16 = 0x09a0;
pub const DC_SYNC1_CYCLE_TIME: u16 = 0x09a4;

pub const DC_SYNC_ACTIVATION_CYCLIC: u8 = 0x01;
pub const DC_SYNC_ACTIVATION_SYNC0: u8 = 0x02;

pub const SPEED_COUNTER_START_DEFAULT: u16 = 0x1000;

#[must_use]
pub const fn fmmu(index: u16) -> u16 {
    FMMU0 + index * FMMU_STRIDE
}

#[must_use]
pub const fn sync_manager(index: u16) -> u16 {
    SM0 + index * SM_STRIDE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum AlState {
    Init = 0x01,
    PreOp = 0x02,
    Boot = 0x03,
    SafeOp = 0x04,
    Op = 0x08,
}

impl AlState {
    pub const ERROR_FLAG: u8 = 0x10;
    pub const STATE_MASK: u8 = 0x0f;

    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code & Self::STATE_MASK {
            0x01 => Some(Self::Init),
            0x02 => Some(Self::PreOp),
            0x03 => Some(Self::Boot),
            0x04 => Some(Self::SafeOp),
            0x08 => Some(Self::Op),
            _ => None,
        }
    }
}

#[must_use]
pub const fn al_status_code_str(code: u16) -> &'static str {
    match code {
        0x0000 => "no error",
        0x0011 => "invalid requested state change",
        0x0012 => "unknown requested state",
        0x0013 => "bootstrap not supported",
        0x0016 => "invalid mailbox configuration",
        0x0017 => "invalid sync manager configuration",
        0x0018 => "no valid inputs available",
        0x0019 => "no valid outputs",
        0x001a => "synchronization error",
        0x001b => "sync manager watchdog",
        0x001c => "invalid sync manager types",
        0x001d => "invalid output configuration",
        0x001e => "invalid input configuration",
        0x001f => "invalid watchdog configuration",
        0x0020 => "subdevice needs cold start",
        0x002c => "fatal sync error",
        0x002d => "no sync error",
        0x0030 => "invalid DC sync configuration",
        0x0031 => "invalid DC latch configuration",
        0x0032 => "PLL error",
        0x0033 => "DC sync IO error",
        0x0034 => "DC sync timeout error",
        0x0035 => "DC invalid sync cycle time",
        0x0036 => "DC sync0 cycle time",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{AlState, FMMU0, SM0, fmmu, sync_manager};

    #[test]
    fn sync_manager_and_fmmu_registers_are_strided() {
        assert_eq!(sync_manager(0), SM0);
        assert_eq!(sync_manager(2), 0x0810);
        assert_eq!(sync_manager(3), 0x0818);
        assert_eq!(fmmu(0), FMMU0);
        assert_eq!(fmmu(2), 0x0620);
    }

    #[test]
    fn al_state_round_trips_and_ignores_the_error_flag() {
        for state in [
            AlState::Init,
            AlState::PreOp,
            AlState::Boot,
            AlState::SafeOp,
            AlState::Op,
        ] {
            assert_eq!(AlState::from_code(state.code()), Some(state));
            assert_eq!(
                AlState::from_code(state.code() | AlState::ERROR_FLAG),
                Some(state)
            );
        }
        assert_eq!(AlState::from_code(0x00), None);
    }
}
