use super::DeviceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceStatus {
    pub state: DeviceState,
    pub ptp_locked: bool,
    pub ptp_failsafe: bool,
}
