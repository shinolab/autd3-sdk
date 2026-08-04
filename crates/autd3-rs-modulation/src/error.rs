use autd3_rs_core::value::SamplingConfigError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum ModulationError {
    #[error("sine modulation value is out of range [0, 255]")]
    SineValueOutOfRange,

    #[error("square duty {duty} must be in range 0..=1")]
    DutyOutOfRange { duty: f32 },

    #[error("fourier components must not be empty")]
    FourierComponentsEmpty,

    #[error("all fourier components must have the same sampling config")]
    FourierSamplingConfigMismatch,

    #[error("fourier modulation value is out of range [0, 255]")]
    FourierValueOutOfRange,

    #[error("modulation sample count exceeds usize")]
    SampleCountOverflow,

    #[error("frequency {hz} Hz is equal to or greater than the Nyquist frequency ({nyquist} Hz)")]
    FrequencyAboveNyquist { hz: f64, nyquist: f32 },

    #[error("modulation frequency must not be zero")]
    FrequencyZero,

    #[error("frequency {hz} Hz must be a valid positive value")]
    FrequencyNotPositive { hz: f32 },

    #[error("frequency {hz} Hz cannot be output with the current sampling config")]
    FrequencyNotRepresentable { hz: f32 },

    #[error("modulation frequency must be a valid value")]
    FrequencyNaN,

    #[error(transparent)]
    SamplingConfig(#[from] SamplingConfigError),
}
