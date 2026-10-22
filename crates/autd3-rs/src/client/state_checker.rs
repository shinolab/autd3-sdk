use std::sync::Weak;

use autd3_rs_core::DeviceStatus;

use super::session::Session;
use crate::udp::UdpError;

#[derive(Clone)]
pub struct StateChecker {
    session: Weak<Session>,
}

impl StateChecker {
    pub(crate) fn new(session: Weak<Session>) -> Self {
        Self { session }
    }

    pub fn check(&self) -> Result<DeviceStatus, UdpError> {
        self.session
            .upgrade()
            .ok_or(UdpError::Closed)?
            .device_status()
    }
}
