crate::wire_enum! {
    pub enum Cmd {
        Reset = 0x00,
        Synchronize = 0x01,
        SetMode = 0x02,
        Clear = 0x03,
        Nop = 0x04,
        Stop = 0x05,
        WritePatternBuffer = 0x10,
        ConfigPattern = 0x11,
        ChangePatternBank = 0x12,
        WritePatternCompressed = 0x13,
        WritePatternFused = 0x14,
        WriteModulationBuffer = 0x20,
        ConfigModulation = 0x21,
        ChangeModulationBank = 0x22,
        WriteModulationFused = 0x23,
        SetSilencer = 0x30,
        SetPhaseCorrection = 0x40,
        SetOutputMask = 0x41,
        SetPulseWidthTable = 0x42,
        EmulateGpioIn = 0x50,
        SetGpioOut = 0x52,
        ForceFan = 0x60,
        ReadErrorDetail = 0xE0,
        ReadCpuFwVersionMajor = 0xE1,
        ReadCpuFwVersionMinor = 0xE2,
        ReadCpuFwVersionPatch = 0xE3,
        ReadFpgaFwVersionMajor = 0xE4,
        ReadFpgaFwVersionMinor = 0xE5,
        ReadFpgaFwVersionPatch = 0xE6,
        ReadFpgaState = 0xE7,
        ReadTelemetry = 0xE8,
        ReadFpgaFunctions = 0xE9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_round_trips() {
        for raw in 0u8..=0xFF {
            if let Some(c) = Cmd::from_u8(raw) {
                assert_eq!(c.as_u8(), raw);
                assert_eq!(Cmd::try_from(raw), Ok(c));
            } else {
                assert_eq!(Cmd::try_from(raw), Err(raw));
            }
        }
    }
}
