use super::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStatus {
    devices: Vec<DeviceState>,
    recoveries: u64,
}

impl LinkStatus {
    #[must_use]
    pub fn new(devices: Vec<DeviceState>, recoveries: u64) -> Self {
        Self {
            devices,
            recoveries,
        }
    }

    #[must_use]
    pub fn op(num_devices: usize) -> Self {
        Self {
            devices: vec![DeviceState::Op; num_devices],
            recoveries: 0,
        }
    }

    #[must_use]
    pub fn devices(&self) -> &[DeviceState] {
        &self.devices
    }

    #[must_use]
    pub fn into_devices(self) -> Vec<DeviceState> {
        self.devices
    }

    #[must_use]
    pub fn recoveries(&self) -> u64 {
        self.recoveries
    }

    pub fn set_recoveries(&mut self, recoveries: u64) {
        self.recoveries = recoveries;
    }

    pub fn set_devices(&mut self, devices: impl IntoIterator<Item = DeviceState>) {
        self.devices.clear();
        self.devices.extend(devices);
    }

    #[must_use]
    pub fn all_op(&self) -> bool {
        self.devices.iter().all(|s| *s == DeviceState::Op)
    }

    #[must_use]
    pub fn any_lost(&self) -> bool {
        self.devices.contains(&DeviceState::Lost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_status_predicates() {
        let status = LinkStatus::op(2);
        assert!(status.all_op());
        assert!(!status.any_lost());

        let status = LinkStatus::new(vec![DeviceState::Op, DeviceState::Lost], 0);
        assert!(!status.all_op());
        assert!(status.any_lost());
    }

    #[test]
    fn set_devices_reuses_the_buffer() {
        let mut status = LinkStatus::op(2);
        status.set_devices([DeviceState::Lost]);
        status.set_recoveries(3);
        assert_eq!(status.devices(), [DeviceState::Lost]);
        assert_eq!(status.recoveries(), 3);
    }
}
