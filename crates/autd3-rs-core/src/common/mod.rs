mod angle;
mod freq;
mod length;
mod velocity;

use core::time::Duration;

pub use angle::Angle;
pub use freq::Freq;
pub use length::Length;
pub use velocity::Velocity;

pub mod units {
    pub use super::angle::{deg, rad};
    pub use super::freq::{Hz, kHz};
    pub use super::length::{m, mm};
    pub use super::velocity::s;
}

use crate::params::ULTRASOUND_FREQ_HZ;

pub const ULTRASOUND_FREQ: Freq<u32> = Freq {
    freq: ULTRASOUND_FREQ_HZ,
};
pub const ULTRASOUND_PERIOD: Duration = Duration::from_micros(25);
