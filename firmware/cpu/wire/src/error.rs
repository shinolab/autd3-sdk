crate::wire_enum_u8! {
    pub enum Error {
        None = 0x00,
        UnknownCmd = 0x01,
        InvalidPayload = 0x02,
        InvalidSilencerSetting = 0x04,
        FpgaTimeout = 0x07,
        SyncNotReady = 0x08,
        UpdateNotStarted = 0x0A,
        UpdateImageInvalid = 0x0B,
        UpdateFlash = 0x0C,
        UpdateNotCommitted = 0x0D,
        UpdateNothingToConfirm = 0x0E,
        UpdateUnsupported = 0x0F,
        FpgaUpdateInProgress = 0x10,
        UpdateActivating = 0x12,
        SyncMissed = 0x14,
        FailsafeConditionActive = 0x15,
    }
}

impl Error {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::None => "no error",
            Self::UnknownCmd => "unknown command (device firmware may be out of date)",
            Self::InvalidPayload => "invalid payload",
            Self::InvalidSilencerSetting => "invalid silencer setting",
            Self::FpgaTimeout => "FPGA did not acknowledge a register update in time",
            Self::SyncNotReady => {
                "the sync pulse is not running (SetTime not received, PTP not locked, or the pulse was re-armed)"
            }
            Self::UpdateNotStarted => "firmware update session is not open (UpdateBegin required)",
            Self::UpdateImageInvalid => "firmware image CRC32 mismatch after write-back",
            Self::UpdateFlash => "serial flash erase/program/read failed",
            Self::UpdateNotCommitted => "no committed firmware image to activate",
            Self::UpdateNothingToConfirm => {
                "the running firmware image is unknown or was overwritten; nothing to confirm"
            }
            Self::UpdateUnsupported => {
                "the running FPGA image cannot write its configuration flash (flash it once via JTAG)"
            }
            Self::FpgaUpdateInProgress => {
                "an FPGA update is in progress; output commands are rejected until the FPGA reboots"
            }
            Self::UpdateActivating => "a reboot is pending; the device resets within 100 ms",
            Self::SyncMissed => {
                "Synchronize missed the sync edge it wrote (the device was too busy to latch in time); send it again"
            }
            Self::FailsafeConditionActive => {
                "the condition that tripped the failsafe still holds; the failsafe stays in effect"
            }
        }
    }
}

#[must_use]
pub fn describe_device_error(code: u8) -> &'static str {
    match Error::from_u8(code) {
        Some(e) => e.describe(),
        None => "unknown error code",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_round_trips() {
        for raw in 0u8..=0xFF {
            match Error::from_u8(raw) {
                Some(e) => {
                    assert_eq!(e.as_u8(), raw);
                    assert_eq!(Error::try_from(raw), Ok(e));
                }
                None => assert_eq!(Error::try_from(raw), Err(raw)),
            }
        }
    }

    #[test]
    fn retired_codes_stay_unassigned() {
        assert_eq!(Error::from_u8(0x03), None);
        assert_eq!(Error::from_u8(0x05), None);
        assert_eq!(Error::from_u8(0x06), None);
        assert_eq!(Error::from_u8(0x09), None);
        assert_eq!(Error::from_u8(0x11), None);
        assert_eq!(Error::from_u8(0x13), None);
        assert_eq!(Error::from_u8(0x16), None);
    }

    #[test]
    fn sync_not_ready_names_every_cause() {
        let text = Error::SyncNotReady.describe();
        for cause in ["SetTime not received", "PTP not locked", "re-armed"] {
            assert!(text.contains(cause));
        }
    }

    #[test]
    fn unknown_code_describes_generically() {
        assert_eq!(describe_device_error(0xFF), "unknown error code");
    }
}
