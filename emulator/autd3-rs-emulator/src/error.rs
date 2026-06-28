use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmulatorError {
    #[error("tick must be a non-zero multiple of the ultrasound period")]
    InvalidTick,
    #[error("duration must be a multiple of the ultrasound period")]
    InvalidDuration,
    #[error("the ultrasound period must be a multiple of the time step")]
    InvalidTimeStep,
    #[error("the requested time range has not been recorded")]
    NotRecorded,
    #[error("{0}")]
    Driver(#[from] autd3_rs::error::Error),
}
