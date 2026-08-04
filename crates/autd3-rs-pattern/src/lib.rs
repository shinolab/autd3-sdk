//! Emission-pattern computation for [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display):
//! [`focus`], [`bessel`], [`plane`], [`null`], and [`uniform`].
//!
//! Each function writes into an emission buffer obtained from the geometry; pass the result to
//! [`autd3-rs`](https://docs.rs/autd3-rs). For multi-focus holograms see
//! [`autd3-rs-pattern-holo`](https://docs.rs/autd3-rs-pattern-holo).
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod bessel;
mod focus;
mod null;
mod plane;
mod uniform;
mod wavelength;

pub use bessel::{BesselOption, bessel, bessel_device, bessel_transducer};
pub use focus::{FocusOption, focus, focus_device, focus_transducer};
pub use null::{null, null_device, null_transducer};
pub use plane::{PlaneOption, plane, plane_device, plane_transducer};
pub use uniform::{uniform, uniform_device};
pub use wavelength::wavelength;
