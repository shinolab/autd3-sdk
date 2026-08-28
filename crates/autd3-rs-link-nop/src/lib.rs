use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::link::IntoLink;
use autd3_rs_firmware_emulator::Audit;

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

    fn into_link(
        self,
        geometry: &Geometry,
    ) -> impl Future<Output = Result<Audit, autd3_rs_core::error::LinkError>> + Send {
        std::future::ready(Ok(Audit::new(geometry.iter().map(Device::num_transducers))))
    }
}
