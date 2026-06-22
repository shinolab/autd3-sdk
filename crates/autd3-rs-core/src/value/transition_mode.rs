#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransitionMode {
    SyncIdx = 0x00,
    SysTime = 0x01,
    Gpio = 0x02,
    Ext = 0xF0,
    #[default]
    Immediate = 0xFF,
}

impl TransitionMode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}
