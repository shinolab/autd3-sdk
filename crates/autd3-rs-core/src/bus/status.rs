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
    pub fn devices(&self) -> &[DeviceState] {
        &self.devices
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
        let status = DeviceStatus::new(vec![DeviceState::Ready; 2]);
        assert!(status.all_ready());
        assert!(!status.any_lost());

        let status = DeviceStatus::new(vec![DeviceState::Ready, DeviceState::Syncing]);
        assert!(!status.all_ready());
        assert!(!status.any_lost());

        let status = DeviceStatus::new(vec![DeviceState::Ready, DeviceState::Lost]);
        assert!(!status.all_ready());
        assert!(status.any_lost());
    }
}
