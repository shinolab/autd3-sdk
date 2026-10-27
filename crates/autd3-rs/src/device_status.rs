use crate::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeviceStatus {
    pub state: DeviceState,
    pub ptp_locked: bool,
    pub failsafe_active: bool,
}
