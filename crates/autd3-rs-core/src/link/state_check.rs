use std::convert::Infallible;

use super::LinkStatus;

pub trait StateCheck: Send + 'static {
    type Error: core::fmt::Display + Send + Sync + 'static;

    fn check(&mut self) -> Result<LinkStatus, Self::Error>;
}

pub struct ConstStateChecker {
    status: LinkStatus,
}

impl ConstStateChecker {
    #[must_use]
    pub fn new(num_devices: usize) -> Self {
        Self {
            status: LinkStatus::op(num_devices),
        }
    }
}

impl StateCheck for ConstStateChecker {
    type Error = Infallible;

    fn check(&mut self) -> Result<LinkStatus, Self::Error> {
        Ok(self.status.clone())
    }
}
