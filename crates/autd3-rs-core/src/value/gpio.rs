#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GpioIn {
    #[default]
    I0 = 0,
    I1 = 1,
    I2 = 2,
    I3 = 3,
}

impl GpioIn {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}
