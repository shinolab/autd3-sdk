use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum ModulationError {
    #[error("modulation value is out of range [0, 255]")]
    ValueOutOfRange,

    #[error("modulation value must not be NaN")]
    ValueNaN,

    #[error("square duty {duty} must be in range 0..=1")]
    DutyOutOfRange { duty: f32 },

    #[error("fourier components must not be empty")]
    FourierComponentsEmpty,

    #[error(
        "the common period of the fourier components exceeds the modulation buffer ({max} samples)"
    )]
    FourierPeriodTooLong { max: usize },

    #[error("frequency {hz} Hz is equal to or greater than the Nyquist frequency ({nyquist} Hz)")]
    FrequencyAboveNyquist { hz: f32, nyquist: f32 },

    #[error("modulation frequency must not be zero")]
    FrequencyZero,

    #[error("frequency {hz} Hz must be a valid positive value")]
    FrequencyNotPositive { hz: f32 },

    #[error("frequency {hz} Hz cannot be output with the current sampling config")]
    FrequencyNotRepresentable { hz: f32 },
}
