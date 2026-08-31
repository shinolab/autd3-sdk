#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PeerVersion {
    pub wire: u8,
    pub sdk: String,
}

impl std::fmt::Display for PeerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wire protocol {} (autd3-sdk {})", self.wire, self.sdk)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteLinkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "remote protocol mismatch: this side speaks {local}, the peer speaks {}. \
         The client SDK and the server image must come from the same release; \
         update the older side (re-flash the appliance image, or install the matching autd3-sdk)",
        peer.as_ref().map_or_else(
            || "an unrecognized protocol".to_owned(),
            ToString::to_string,
        )
    )]
    ProtocolMismatch {
        local: PeerVersion,
        peer: Option<PeerVersion>,
    },
    #[error("unexpected message tag {0:#04x}")]
    UnexpectedTag(u8),
    #[error("invalid device count {found} negotiated during handshake")]
    InvalidDeviceCount { found: usize },
    #[error("the bus is closed")]
    BusClosed,
    #[error("the bus is unavailable: {reason}")]
    BusUnavailable { reason: String },
    #[error(
        "the client geometry has {client} device(s) but the bus has {bus}; \
         build the geometry from what is actually wired to the appliance"
    )]
    GeometryMismatch { client: usize, bus: usize },
    #[error("the bus device count changed from {expected} to {found} while the session was open")]
    DeviceCountChanged { expected: usize, found: usize },
    #[error(
        "the bus was opened for a client before the probe could run; \
         close the bus first, or read the device count from the running bus"
    )]
    ProbeBusOpened,
    #[error("the probe did not finish within {}s", timeout.as_secs())]
    ProbeTimeout { timeout: std::time::Duration },
    #[error("the server refused the session ({kind}): {detail}")]
    SessionRejected { kind: RejectKind, detail: String },
    #[error("inner link error: {0}")]
    Link(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectKind {
    BusClosed,
    DeviceCount,
    BusUnavailable,
    Internal,
    Unknown(u8),
}

impl std::fmt::Display for RejectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BusClosed => f.write_str("the bus is closed"),
            Self::DeviceCount => f.write_str("device count mismatch"),
            Self::BusUnavailable => f.write_str("the bus is unavailable"),
            Self::Internal => f.write_str("server error"),
            Self::Unknown(code) => write!(f, "unknown reason {code:#04x}"),
        }
    }
}
