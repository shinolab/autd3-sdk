use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("device {device} reported error flag ({code:#04x})")]
    DeviceError { device: usize, code: u8 },

    #[error("ack timeout after {cycles} cycles")]
    Timeout { cycles: u32 },

    #[error("link error: {0}")]
    Link(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("client RT worker is no longer alive")]
    RtClosed,
}
