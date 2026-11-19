mod angle;
mod length;
mod velocity;

pub use angle::Angle;
pub use autd3_cpu_wire::value::{Freq, ULTRASOUND_PERIOD};
pub use length::Length;
pub use velocity::Velocity;

pub mod units {
    pub use super::angle::{deg, rad};
    pub use super::length::{m, mm};
    pub use super::velocity::s;
    pub use autd3_cpu_wire::value::{Hz, kHz};
}
