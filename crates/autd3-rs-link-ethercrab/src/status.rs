use ethercrab::{Command, MainDevice, RegisterAddress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpRecoveryAction {
    None,
    AckSafeOpError,
    RequestOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AlState(pub(crate) u16);

impl AlState {
    const STATE_MASK: u16 = 0x0f;
    const ERROR_FLAG: u16 = 0x10;
    pub(crate) const SAFE_OP: u16 = 0x04;
    pub(crate) const OP: u16 = 0x08;
    pub(crate) const SAFE_OP_ACK: u16 = Self::SAFE_OP | Self::ERROR_FLAG;

    pub(crate) fn is_op(self) -> bool {
        self.0 & Self::STATE_MASK == Self::OP
    }

    pub(crate) fn is_safe_op(self) -> bool {
        self.0 & Self::STATE_MASK == Self::SAFE_OP
    }

    pub(crate) fn is_error(self) -> bool {
        self.0 & Self::ERROR_FLAG != 0
    }

    pub(crate) fn op_recovery_action(self) -> OpRecoveryAction {
        if self.is_safe_op() && self.is_error() {
            OpRecoveryAction::AckSafeOpError
        } else if self.is_safe_op() {
            OpRecoveryAction::RequestOp
        } else {
            OpRecoveryAction::None
        }
    }

    pub(crate) fn state_bits(self) -> u8 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.0 & Self::STATE_MASK) as u8
        }
    }
}

pub(crate) async fn read_al_state(
    maindevice: &MainDevice<'_>,
    address: u16,
) -> Result<AlState, ethercrab::error::Error> {
    Command::fprd(address, RegisterAddress::AlStatus.into())
        .receive::<u16>(maindevice)
        .await
        .map(AlState)
}

pub(crate) async fn read_al_status_code(
    maindevice: &MainDevice<'_>,
    address: u16,
) -> Result<u16, ethercrab::error::Error> {
    Command::fprd(address, RegisterAddress::AlStatusCode.into())
        .receive::<u16>(maindevice)
        .await
}

pub(crate) async fn request_al_state(
    maindevice: &MainDevice<'_>,
    address: u16,
    control: u16,
) -> Result<(), ethercrab::error::Error> {
    Command::fpwr(address, RegisterAddress::AlControl.into())
        .send_receive::<u16>(maindevice, control)
        .await
        .map(|_: u16| ())
}

pub(crate) const fn al_status_code_str(code: u16) -> &'static str {
    match code {
        0x0000 => "no error",
        0x0011 => "invalid requested state change",
        0x0012 => "unknown requested state",
        0x0017 => "invalid sync manager configuration",
        0x0018 => "no valid inputs available",
        0x0019 => "no valid outputs",
        0x001A => "synchronization error",
        0x001B => "sync manager watchdog",
        0x001D => "invalid output configuration",
        0x001E => "invalid input configuration",
        0x0024 => "invalid input mapping",
        0x0025 => "invalid output mapping",
        0x0028 => "sync mode not supported",
        0x002C => "fatal sync error",
        0x002D => "no sync error (sync signal missing)",
        0x0030 => "invalid DC SYNC configuration",
        0x0031 => "invalid DC latch configuration",
        0x0032 => "DC PLL error",
        0x0033 => "DC sync IO error",
        0x0034 => "DC sync timeout error",
        0x0035 => "DC invalid sync cycle time",
        0x0036 => "DC Sync0 cycle time",
        0x0037 => "DC Sync1 cycle time",
        _ => "see ETG.1000 AL status codes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn al_state_decodes_status_word() {
        assert!(AlState(0x08).is_op());
        assert!(AlState(0x04).is_safe_op());
        assert!(!AlState(0x14).is_op());
        assert!(AlState(0x14).is_safe_op());
        assert!(AlState(0x14).is_error());
        assert_eq!(AlState(0x12).state_bits(), 0x02);
    }

    #[test]
    fn al_state_reports_op_recovery_action() {
        assert_eq!(AlState(0x08).op_recovery_action(), OpRecoveryAction::None);
        assert_eq!(
            AlState(0x04).op_recovery_action(),
            OpRecoveryAction::RequestOp
        );
        assert_eq!(
            AlState(0x14).op_recovery_action(),
            OpRecoveryAction::AckSafeOpError
        );
    }
}
