use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
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
    Autd3(#[from] autd3_rs::error::Error),
    #[cfg(feature = "gpu")]
    #[error("{0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),
    #[cfg(feature = "gpu")]
    #[error("{0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[cfg(feature = "gpu")]
    #[error("{0}")]
    BufferAsync(#[from] wgpu::BufferAsyncError),
}
