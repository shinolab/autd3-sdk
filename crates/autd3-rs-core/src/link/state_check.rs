use std::convert::Infallible;

use super::LinkStatus;

pub trait StateCheck: Send + 'static {
    type Error: core::fmt::Display + Send + Sync + 'static;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send;
}

pub struct ConstStateChecker {
    status: LinkStatus,
}

impl ConstStateChecker {
    #[must_use]
    pub fn new(num_devices: usize) -> Self {
        Self {
            status: LinkStatus::new(num_devices),
        }
    }
}

impl StateCheck for ConstStateChecker {
    type Error = Infallible;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        std::future::ready(Ok(self.status.clone()))
    }
}
