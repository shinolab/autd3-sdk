#[derive(Debug, thiserror::Error)]
pub enum RemoteLinkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol handshake mismatch: unexpected magic or version")]
    ProtocolMismatch,
    #[error("unexpected message tag {0:#04x}")]
    UnexpectedTag(u8),
    #[error("invalid device count {found} negotiated during handshake")]
    InvalidDeviceCount { found: usize },
    #[error("inner link error: {0}")]
    Link(String),
}
