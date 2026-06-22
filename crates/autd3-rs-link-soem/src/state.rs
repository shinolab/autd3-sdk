#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlState(pub(crate) u16);

impl AlState {
    const STATE_MASK: u16 = 0x0f;
    const ERROR_FLAG: u16 = 0x10;
    pub(crate) const NONE: Self = Self(0x00);
    pub(crate) const INIT: Self = Self(0x01);
    pub(crate) const SAFE_OP: Self = Self(0x04);
    pub(crate) const OP: Self = Self(0x08);
    pub(crate) const SAFE_OP_ACK: Self = Self(0x04 | Self::ERROR_FLAG);

    pub(crate) fn is_op(self) -> bool {
        self.0 & Self::STATE_MASK == Self::OP.0
    }

    pub(crate) fn is_safe_op(self) -> bool {
        self.0 & Self::STATE_MASK == Self::SAFE_OP.0
    }

    pub(crate) fn is_none(self) -> bool {
        self.0 & Self::STATE_MASK == Self::NONE.0
    }

    pub(crate) fn is_error(self) -> bool {
        self.0 & Self::ERROR_FLAG != 0
    }

    pub(crate) fn state_bits(self) -> u8 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.0 & Self::STATE_MASK) as u8
        }
    }
}

impl std::fmt::Display for AlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 & Self::STATE_MASK {
            0x00 => write!(f, "NONE")?,
            0x01 => write!(f, "INIT")?,
            0x02 => write!(f, "PRE-OP")?,
            0x03 => write!(f, "BOOT")?,
            0x04 => write!(f, "SAFE-OP")?,
            0x08 => write!(f, "OP")?,
            bits => write!(f, "UNKNOWN ({bits:#04x})")?,
        }
        if self.is_error() {
            write!(f, " + ERROR")?;
        }
        Ok(())
    }
}
