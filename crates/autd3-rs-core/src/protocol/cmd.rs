#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Reset = 0x00,
    Synchronize = 0x01,
    SetMode = 0x02,

    WritePatternBuffer = 0x10,
    ConfigPattern = 0x11,
    ChangePatternBank = 0x12,

    WriteModulationBuffer = 0x20,
    ConfigModulation = 0x21,
    ChangeModulationBank = 0x22,

    SetSilencer = 0x30,

    ReadErrorDetail = 0xE0,
    ReadCpuFwVersionMajor = 0xE1,
    ReadCpuFwVersionMinor = 0xE2,
    ReadCpuFwVersionPatch = 0xE3,

    XorHash = 0xF0,
}

impl Cmd {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for Cmd {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        match value {
            0x00 => Ok(Self::Reset),
            0x01 => Ok(Self::Synchronize),
            0x02 => Ok(Self::SetMode),
            0x10 => Ok(Self::WritePatternBuffer),
            0x11 => Ok(Self::ConfigPattern),
            0x12 => Ok(Self::ChangePatternBank),
            0x20 => Ok(Self::WriteModulationBuffer),
            0x21 => Ok(Self::ConfigModulation),
            0x22 => Ok(Self::ChangeModulationBank),
            0x30 => Ok(Self::SetSilencer),
            0xE0 => Ok(Self::ReadErrorDetail),
            0xE1 => Ok(Self::ReadCpuFwVersionMajor),
            0xE2 => Ok(Self::ReadCpuFwVersionMinor),
            0xE3 => Ok(Self::ReadCpuFwVersionPatch),
            0xF0 => Ok(Self::XorHash),
            other => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_round_trips_via_try_from() {
        for c in [
            Cmd::Reset,
            Cmd::Synchronize,
            Cmd::SetMode,
            Cmd::WritePatternBuffer,
            Cmd::ConfigPattern,
            Cmd::ChangePatternBank,
            Cmd::WriteModulationBuffer,
            Cmd::ConfigModulation,
            Cmd::ChangeModulationBank,
            Cmd::SetSilencer,
            Cmd::ReadErrorDetail,
            Cmd::ReadCpuFwVersionMajor,
            Cmd::ReadCpuFwVersionMinor,
            Cmd::ReadCpuFwVersionPatch,
            Cmd::XorHash,
        ] {
            assert_eq!(Cmd::try_from(c.as_u8()), Ok(c));
        }
    }
}
