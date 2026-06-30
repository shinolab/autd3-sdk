#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceErrorCode {
    None = 0x00,
    UnknownCmd = 0x01,
    InvalidPayload = 0x02,
    InvalidData = 0x03,
    InvalidSilencerSetting = 0x04,
    InvalidTransitionMode = 0x05,
    MissTransitionTime = 0x06,
}

impl DeviceErrorCode {
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
        }
    }
}

impl TryFrom<u8> for DeviceErrorCode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        match value {
            0x00 => Ok(Self::None),
            0x01 => Ok(Self::UnknownCmd),
            0x02 => Ok(Self::InvalidPayload),
            0x03 => Ok(Self::InvalidData),
            0x04 => Ok(Self::InvalidSilencerSetting),
            0x05 => Ok(Self::InvalidTransitionMode),
            0x06 => Ok(Self::MissTransitionTime),
            other => Err(other),
        }
    }
}

#[must_use]
pub fn describe_device_error(code: u8) -> &'static str {
    match DeviceErrorCode::try_from(code) {
        Ok(c) => c.describe(),
        Err(_) => "unknown error code",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_error_code_round_trips() {
        for c in [
            DeviceErrorCode::None,
            DeviceErrorCode::UnknownCmd,
            DeviceErrorCode::InvalidPayload,
            DeviceErrorCode::InvalidData,
            DeviceErrorCode::InvalidSilencerSetting,
            DeviceErrorCode::InvalidTransitionMode,
            DeviceErrorCode::MissTransitionTime,
        ] {
            assert_eq!(DeviceErrorCode::try_from(c as u8), Ok(c));
        }
    }

    #[test]
    fn unknown_code_describes_generically() {
        assert_eq!(describe_device_error(0xFF), "unknown error code");
    }
}
