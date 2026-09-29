#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Op,
    SafeOp,
    SafeOpError,
    Lost,
    Other(u8),
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Op => write!(f, "OP"),
            DeviceState::SafeOp => write!(f, "SAFE-OP"),
            DeviceState::SafeOpError => write!(f, "SAFE-OP + ERROR"),
            DeviceState::Lost => write!(f, "LOST"),
            DeviceState::Other(bits) => match bits {
                0x00 => write!(f, "NONE"),
                0x01 => write!(f, "INIT"),
                0x02 => write!(f, "PRE-OP"),
                0x03 => write!(f, "BOOT"),
                bits => write!(f, "UNKNOWN ({bits:#04x})"),
            },
        }
    }
}
