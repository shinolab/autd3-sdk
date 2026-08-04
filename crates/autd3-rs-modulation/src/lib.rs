//! Amplitude-modulation waveforms for
//! [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display): [`sine`], [`square`],
//! [`constant`], and [`fourier`].
//!
//! Each function writes into a [`modulation_buffer`]; pass the result to
//! [`autd3-rs`](https://docs.rs/autd3-rs).
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod constant;
mod error;
mod fourier;
mod radiation_pressure;
mod sampling;
mod sampling_mode;
mod sine;
mod square;

use autd3_rs_core::params::MOD_BUFFER_SAMPLES;

#[must_use]
pub fn modulation_buffer() -> Vec<u8> {
    Vec::with_capacity(MOD_BUFFER_SAMPLES)
}

pub use autd3_rs_core::value::Nearest;
pub use constant::constant;
pub use error::ModulationError;
pub use fourier::{FourierOption, SineComponent, fourier};
pub use radiation_pressure::{radiation_pressure, radiation_pressure_inplace};
pub use sampling::samples_per_period;
pub use sampling_mode::SamplingMode;
pub use sine::{SineOption, sine};
pub use square::{SquareOption, square};
