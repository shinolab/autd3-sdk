use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridAxis {
    X,
    Y,
    Z,
}

impl std::fmt::Display for GridAxis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        })
    }
}

#[derive(Error, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidGrid {
    #[error("resolution must be finite and greater than zero")]
    Resolution,
    #[error("the bounds of the {0} axis must be finite")]
    NonFiniteBound(GridAxis),
    #[error("the start of the {0} axis must not exceed its end")]
    Reversed(GridAxis),
    #[error("the number of points is too large")]
    TooManyPoints,
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum EmulatorError {
    #[error("invalid grid: {0}")]
    InvalidGrid(#[from] InvalidGrid),
    #[error("tick must be a non-zero multiple of the ultrasound period")]
    InvalidTick,
    #[error("duration must be a multiple of the ultrasound period")]
    InvalidDuration,
    #[error("the ultrasound period must be a multiple of the time step")]
    InvalidTimeStep,
    #[error("the requested time range has not been recorded")]
    NotRecorded,
    #[error("{0}")]
    Autd3(#[from] autd3_rs::error::Error),
}
