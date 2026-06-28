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
pub use fourier::{FourierOption, SineComponent, fourier};
pub use radiation_pressure::{radiation_pressure, radiation_pressure_inplace};
pub use sampling::samples_per_period;
pub use sampling_mode::SamplingMode;
pub use sine::{SineOption, sine};
pub use square::{SquareOption, square};
