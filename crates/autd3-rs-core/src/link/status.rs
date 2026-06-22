use super::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStatus {
    pub devices: Vec<DeviceState>,
    pub recoveries: u64,
}

impl LinkStatus {
    #[must_use]
    pub fn new(num_devices: usize) -> Self {
        Self {
            devices: vec![DeviceState::Op; num_devices],
            recoveries: 0,
        }
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
        let status = LinkStatus::new(2);
        assert!(status.all_op());
        assert!(!status.any_lost());

        let status = LinkStatus {
            devices: vec![DeviceState::Op, DeviceState::Lost],
            recoveries: 0,
        };
        assert!(!status.all_op());
        assert!(status.any_lost());
    }
}
