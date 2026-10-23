use zerocopy::{FromBytes, Immutable, IntoBytes};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromBytes, IntoBytes, Immutable)]
pub struct Intensity(pub u8);

impl Default for Intensity {
    fn default() -> Self {
        Self::MAX
    }
}

impl core::fmt::Debug for Intensity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{:02X}", self.0)
    }
}

impl Intensity {
    pub const MAX: Intensity = Intensity(0xFF);
    pub const MIN: Intensity = Intensity(0x00);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", Intensity(0x00)), "0x00");
        assert_eq!(format!("{:?}", Intensity(0xFF)), "0xFF");
    }
}
