crate::wire_enum! {
    pub enum Cmd {
        Reset = 0x00,
        Synchronize = 0x01,
        SetMode = 0x02,
        Clear = 0x03,
        Nop = 0x04,
        WriteFociBuffer = 0x10,
        ConfigPattern = 0x11,
        ActivatePatternBank = 0x12,
        WritePatternPhase = 0x13,
        WritePatternRaw = 0x15,
        WriteModulationBuffer = 0x20,
        ConfigModulation = 0x21,
        ActivateModulationBank = 0x22,
        SetSilencer = 0x30,
        SetPhaseCorrection = 0x40,
        SetOutputMask = 0x41,
        SetPulseWidthTable = 0x42,
        EmulateGpioIn = 0x50,
        SetGpioOut = 0x52,
        ForceFan = 0x60,
        UpdateBegin = 0x70,
        UpdateChunk = 0x71,
        UpdateCommit = 0x72,
        UpdateActivate = 0x73,
        UpdateConfirm = 0x74,
        FpgaUpdateBegin = 0x75,
        FpgaUpdateChunk = 0x76,
        FpgaUpdateCommit = 0x77,
        FpgaUpdateActivate = 0x78,
        ReadErrorDetail = 0xE0,
        ReadFpgaState = 0xE7,
        ReadTelemetry = 0xE8,
        ReadFirmwareInfo = 0xEB,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_read_ids_stay_unassigned() {
        for raw in [0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE9, 0xEA] {
            assert_eq!(Cmd::from_u8(raw), None);
        }
    }

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
