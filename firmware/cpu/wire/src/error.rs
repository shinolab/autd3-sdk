crate::wire_enum! {
    pub enum Error {
        None = 0x00,
        UnknownCmd = 0x01,
        InvalidPayload = 0x02,
        InvalidData = 0x03,
        InvalidSilencerSetting = 0x04,
        InvalidTransitionMode = 0x05,
        MissTransitionTime = 0x06,
        FpgaTimeout = 0x07,
        SyncNotReady = 0x08,
        InvalidSync0Cycle = 0x09,
    }
}

impl Error {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::None => "no error",
            Self::UnknownCmd => "unknown command (device firmware may be out of date)",
            Self::InvalidPayload => "invalid payload",
            Self::InvalidData => "invalid data",
            Self::InvalidSilencerSetting => "invalid silencer setting",
            Self::InvalidTransitionMode => "invalid transition mode for the target loop behavior",
            Self::MissTransitionTime => "sys-time transition is too close to now (would be missed)",
            Self::FpgaTimeout => "FPGA did not acknowledge a register update in time",
            Self::SyncNotReady => "EtherCAT DC is not configured (no SYNC0 time available)",
            Self::InvalidSync0Cycle => {
                "invalid Sync0 cycle time (master's DC config missing or not a multiple of 500us)"
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
    fn unknown_code_describes_generically() {
        assert_eq!(describe_device_error(0xFF), "unknown error code");
    }
}
