use super::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    devices: Vec<DeviceState>,
}

impl DeviceStatus {
    #[must_use]
    pub fn new(devices: Vec<DeviceState>) -> Self {
        Self { devices }
    }

    #[must_use]
    pub fn ready(num_devices: usize) -> Self {
        Self {
            devices: vec![DeviceState::Ready; num_devices],
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

    pub fn set_devices(&mut self, devices: impl IntoIterator<Item = DeviceState>) {
        self.devices.clear();
        self.devices.extend(devices);
    }

    #[must_use]
    pub fn all_ready(&self) -> bool {
        self.devices.iter().all(|s| *s == DeviceState::Ready)
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
    fn status_predicates() {
        let status = DeviceStatus::ready(2);
        assert!(status.all_ready());
        assert!(!status.any_lost());

        let status = DeviceStatus::new(vec![DeviceState::Ready, DeviceState::Syncing]);
        assert!(!status.all_ready());
        assert!(!status.any_lost());

        let status = DeviceStatus::new(vec![DeviceState::Ready, DeviceState::Lost]);
        assert!(!status.all_ready());
        assert!(status.any_lost());
    }

    #[test]
    fn set_devices_reuses_the_buffer() {
        let mut status = DeviceStatus::ready(2);
        status.set_devices([DeviceState::Lost]);
        assert_eq!(status.devices(), [DeviceState::Lost]);
    }
}
