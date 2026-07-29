#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Nop = 0x00,
    Clear = 0x01,
    Sync = 0x02,
    FirmInfo = 0x03,
    Modulation = 0x10,
    ModulationLegacyChangePatternBank = 0x11,
    Silencer = 0x21,
    Gain = 0x30,
    GainLegacyChangePatternBank = 0x31,
    GainStm = 0x41,
    FociStm = 0x42,
    GainStmLegacyChangePatternBank = 0x43,
    FociStmLegacyChangePatternBank = 0x44,
    ForceFan = 0x60,
    ReadsFpgaState = 0x61,
    ConfigPulseWidthEncoder = 0x72,
    PhaseCorrection = 0x80,
    OutputMask = 0x90,
    FpgaGpioOut = 0xF0,
    EmulateGpioIn = 0xF1,
}

impl Tag {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InfoType {
    CpuMajor = 0x01,
    CpuMinor = 0x02,
    FpgaMajor = 0x03,
    FpgaMinor = 0x04,
    FpgaFunctions = 0x05,
    Clear = 0x06,
}

impl InfoType {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GainStmMode {
    #[default]
    PhaseIntensityFull = 0,
    PhaseFull = 1,
    PhaseHalf = 2,
}

impl GainStmMode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn frames_per_round(self) -> usize {
        match self {
            GainStmMode::PhaseIntensityFull => 1,
            GainStmMode::PhaseFull => 2,
            GainStmMode::PhaseHalf => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_values_match_legacy_firmware() {
        assert_eq!(Tag::Nop.as_u8(), 0x00);
        assert_eq!(Tag::Clear.as_u8(), 0x01);
        assert_eq!(Tag::Sync.as_u8(), 0x02);
        assert_eq!(Tag::FirmInfo.as_u8(), 0x03);
        assert_eq!(Tag::Modulation.as_u8(), 0x10);
        assert_eq!(Tag::ModulationLegacyChangePatternBank.as_u8(), 0x11);
        assert_eq!(Tag::Silencer.as_u8(), 0x21);
        assert_eq!(Tag::Gain.as_u8(), 0x30);
        assert_eq!(Tag::GainLegacyChangePatternBank.as_u8(), 0x31);
        assert_eq!(Tag::GainStm.as_u8(), 0x41);
        assert_eq!(Tag::FociStm.as_u8(), 0x42);
        assert_eq!(Tag::GainStmLegacyChangePatternBank.as_u8(), 0x43);
        assert_eq!(Tag::FociStmLegacyChangePatternBank.as_u8(), 0x44);
        assert_eq!(Tag::ForceFan.as_u8(), 0x60);
        assert_eq!(Tag::ReadsFpgaState.as_u8(), 0x61);
        assert_eq!(Tag::ConfigPulseWidthEncoder.as_u8(), 0x72);
        assert_eq!(Tag::PhaseCorrection.as_u8(), 0x80);
        assert_eq!(Tag::OutputMask.as_u8(), 0x90);
        assert_eq!(Tag::FpgaGpioOut.as_u8(), 0xF0);
        assert_eq!(Tag::EmulateGpioIn.as_u8(), 0xF1);
    }

    #[test]
    fn gain_stm_mode_frames_per_round() {
        assert_eq!(GainStmMode::PhaseIntensityFull.frames_per_round(), 1);
        assert_eq!(GainStmMode::PhaseFull.frames_per_round(), 2);
        assert_eq!(GainStmMode::PhaseHalf.frames_per_round(), 4);
    }
}
