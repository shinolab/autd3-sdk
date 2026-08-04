//! Hardware-free `Link` for [`autd3-rs`](https://docs.rs/autd3-rs), backed by
//! [`autd3-rs-firmware-emulator`](https://docs.rs/autd3-rs-firmware-emulator).
//!
//! Frames go to an emulated device instead of the bus, so examples, tests, and documentation
//! samples run without an AUTD3 connected.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

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

    async fn into_link(
        self,
        geometry: &Geometry,
    ) -> Result<Audit, autd3_rs_core::error::LinkError> {
        Ok(Audit::new(geometry.iter().map(Device::num_transducers)))
    }
}
