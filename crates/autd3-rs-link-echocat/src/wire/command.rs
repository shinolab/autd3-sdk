#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Nop = 0x00,
    Aprd = 0x01,
    Apwr = 0x02,
    Aprw = 0x03,
    Fprd = 0x04,
    Fpwr = 0x05,
    Fprw = 0x06,
    Brd = 0x07,
    Bwr = 0x08,
    Brw = 0x09,
    Lrd = 0x0a,
    Lwr = 0x0b,
    Lrw = 0x0c,
    Armw = 0x0d,
    Frmw = 0x0e,
}

impl Command {
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => Some(Self::Nop),
            0x01 => Some(Self::Aprd),
            0x02 => Some(Self::Apwr),
            0x03 => Some(Self::Aprw),
            0x04 => Some(Self::Fprd),
            0x05 => Some(Self::Fpwr),
            0x06 => Some(Self::Fprw),
            0x07 => Some(Self::Brd),
            0x08 => Some(Self::Bwr),
            0x09 => Some(Self::Brw),
            0x0a => Some(Self::Lrd),
            0x0b => Some(Self::Lwr),
            0x0c => Some(Self::Lrw),
            0x0d => Some(Self::Armw),
            0x0e => Some(Self::Frmw),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn command_codes_match_the_wire_encoding() {
        assert_eq!(Command::Nop.code(), 0);
        assert_eq!(Command::Aprd.code(), 1);
        assert_eq!(Command::Apwr.code(), 2);
        assert_eq!(Command::Fprd.code(), 4);
        assert_eq!(Command::Fpwr.code(), 5);
        assert_eq!(Command::Brd.code(), 7);
        assert_eq!(Command::Bwr.code(), 8);
        assert_eq!(Command::Lrd.code(), 10);
        assert_eq!(Command::Lwr.code(), 11);
        assert_eq!(Command::Lrw.code(), 12);
        assert_eq!(Command::Armw.code(), 13);
        assert_eq!(Command::Frmw.code(), 14);
    }
}
