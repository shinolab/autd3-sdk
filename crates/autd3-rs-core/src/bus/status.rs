use super::{DeviceMask, DeviceState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    devices: Vec<DeviceState>,
    ptp_locked: DeviceMask,
    ptp_failsafe: DeviceMask,
}

impl DeviceStatus {
    #[must_use]
    pub fn new(
        devices: Vec<DeviceState>,
        ptp_locked: DeviceMask,
        ptp_failsafe: DeviceMask,
    ) -> Self {
        Self {
            devices,
            ptp_locked,
            ptp_failsafe,
        }
    }

    #[must_use]
    pub fn devices(&self) -> &[DeviceState] {
        &self.devices
    }

    #[must_use]
    pub fn ptp_locked(&self) -> DeviceMask {
        self.ptp_locked
    }

    #[must_use]
    pub fn ptp_failsafe(&self) -> DeviceMask {
        self.ptp_failsafe
    }

    #[must_use]
    pub fn all_ready(&self) -> bool {
        self.devices.iter().all(|s| *s == DeviceState::Ready)
    }

    #[must_use]
    pub fn any_lost(&self) -> bool {
        self.devices.contains(&DeviceState::Lost)
    }

    #[must_use]
    pub fn any_ptp_failsafe(&self) -> bool {
        !self.ptp_failsafe.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn without_ptp(devices: Vec<DeviceState>) -> DeviceStatus {
        DeviceStatus::new(devices, DeviceMask::default(), DeviceMask::default())
    }

    #[test]
    fn status_predicates() {
        let status = without_ptp(vec![DeviceState::Ready; 2]);
        assert!(status.all_ready());
        assert!(!status.any_lost());

        let status = without_ptp(vec![DeviceState::Ready, DeviceState::Syncing]);
        assert!(!status.all_ready());
        assert!(!status.any_lost());

        let status = without_ptp(vec![DeviceState::Ready, DeviceState::Lost]);
        assert!(!status.all_ready());
        assert!(status.any_lost());
    }

    #[test]
    fn the_ptp_masks_are_reported_as_given() {
        let status = DeviceStatus::new(
            vec![DeviceState::Ready, DeviceState::Syncing],
            DeviceMask::from_bits(0b01),
            DeviceMask::from_bits(0b10),
        );
        assert_eq!(status.ptp_locked(), DeviceMask::from_bits(0b01));
        assert_eq!(status.ptp_failsafe(), DeviceMask::from_bits(0b10));
        assert!(status.any_ptp_failsafe());
        assert!(!without_ptp(vec![DeviceState::Ready]).any_ptp_failsafe());
    }
}
