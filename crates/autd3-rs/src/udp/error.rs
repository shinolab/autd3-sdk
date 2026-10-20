use std::net::SocketAddrV6;
use std::time::Duration;

use autd3_cpu_wire::udp::Kind;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UdpError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("no interface has an AUTD3 device answering ReadUnitInfo")]
    NoInterfaceFound,
    #[error("AUTD3 devices answered on several destinations ({}); choose one with `iface`", .0.join(", "))]
    AmbiguousInterface(Vec<String>),
    #[error(
        "the simulator port must be within 1..=65534 (its control port is port + 1), but {0} was given"
    )]
    InvalidSimulatorPort(u16),
    #[error("no simulator answers at {0}")]
    SimulatorNotFound(SocketAddrV6),
    #[error(
        "the simulator did not answer the geometry at {0} within open_timeout; is it an older simulator?"
    )]
    SimulatorControlNoResponse(SocketAddrV6),
    #[error("the simulator refused the geometry with status {status:#04x}")]
    SimulatorRejected { status: u8 },
    #[error("interface {0} does not exist or has no IPv6 link-local address")]
    InterfaceNotFound(String),
    #[error(
        "the device speaks UDP protocol version {device_version}, but this host speaks version {host_version}"
    )]
    UnsupportedVersion {
        device_version: u8,
        host_version: u8,
    },
    #[error("unit {unit_id} rejected {kind:?} with status {status:#04x}")]
    Rejected { kind: Kind, unit_id: u8, status: u8 },
    #[error("unit {unit_id} did not answer {kind:?}")]
    NoResponse { kind: Kind, unit_id: u8 },
    #[error(
        "{replies} unassigned devices answered one Discover; a device failed to close its downstream port"
    )]
    DownstreamNotClosed { replies: usize },
    #[error(
        "only {found} of the {expected} devices declared by the geometry were enumerated within open_timeout"
    )]
    EnumerationTimeout { expected: usize, found: usize },
    #[error("geometry declares {expected} devices but {found} are attached")]
    DeviceCountMismatch { expected: usize, found: usize },
    #[error("an unexpected unit (id {unit_id:#04x}) at {addr} answered the cross-check")]
    UnexpectedUnit { unit_id: u8, addr: SocketAddrV6 },
    #[error("unit {0} did not answer the cross-check")]
    MissingUnit(u8),
    #[error(
        "units {not_ready:?} did not lock to the grandmaster and start their sync pulse within {timeout:?}"
    )]
    SyncTimeout {
        not_ready: Vec<u8>,
        timeout: Duration,
    },
    #[error("open_timeout must be longer than zero")]
    ZeroOpenTimeout,
    #[error("recovery_reply_timeout must be longer than zero")]
    ZeroRecoveryReplyTimeout,
    #[error("lost_timeout must be longer than zero")]
    ZeroLostTimeout,
    #[error("ack_timeout must be longer than zero")]
    ZeroAckTimeout,
    #[error("send_rate_limit_percent must be within {min}..=100 %, but {value} % was given")]
    InvalidSendRateLimit { value: f32, min: f32 },
    #[error("the number of devices must be 1..=128, but {0} was given")]
    InvalidDeviceCount(usize),
    #[error("{expected} frames are needed (one per device), but {got} were given")]
    FrameCountMismatch { expected: usize, got: usize },
    #[error("a frame must be 6..=1448 bytes, but {0} bytes were given")]
    InvalidFrameLength(usize),
    #[error("the frame could not be sent to device {device}: {source}")]
    Unsent {
        device: usize,
        source: std::io::Error,
    },
    #[error("the connection is closed")]
    Closed,
}
