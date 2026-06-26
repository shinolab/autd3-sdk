use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::link::IntoLink;

use crate::audit::Audit;

#[derive(Clone, Copy, Debug, Default)]
pub struct Nop;

impl Nop {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IntoLink for Nop {
    type Link = Audit;

    async fn into_link(self, geometry: &Geometry) -> Result<Audit, autd3_rs_core::Error> {
        Ok(Audit::new(geometry.iter().map(Device::len)))
    }
}
