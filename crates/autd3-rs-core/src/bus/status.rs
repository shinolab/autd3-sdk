use super::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    pub state: DeviceState,
    pub ptp_locked: bool,
    pub failsafe_active: bool,
}
