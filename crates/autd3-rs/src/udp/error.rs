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
    #[error("AUTD3 devices answered on several interfaces ({}); choose one with `iface`", .0.join(", "))]
    AmbiguousInterface(Vec<String>),
    #[error("interface {0} does not exist or has no IPv6 link-local address")]
    InterfaceNotFound(String),
    #[error("the device speaks UDP protocol version {device}, but this host speaks version {host}")]
    UnsupportedVersion { device: u8, host: u8 },
    #[error("unit {unit} rejected {kind:?} with status {status:#04x}")]
    Rejected { kind: Kind, unit: u8, status: u8 },
    #[error("unit {unit} did not answer {kind:?}")]
    NoResponse { kind: Kind, unit: u8 },
    #[error(
        "{replies} unassigned devices answered one Discover; a device failed to close its downstream port"
    )]
    DownstreamNotClosed { replies: usize },
    #[error("only {found} of the {expected} devices declared by the geometry were enumerated")]
    EnumerationTimeout { expected: usize, found: usize },
    #[error("geometry declares {expected} devices but {found} are attached")]
    DeviceCountMismatch { expected: usize, found: usize },
    #[error("an unexpected unit (id {unit_id:#04x}) at {addr} answered the cross-check")]
    UnexpectedUnit { unit_id: u8, addr: SocketAddrV6 },
    #[error("unit {0} did not answer the cross-check")]
    MissingUnit(u8),
    #[error("units {not_ready:?} did not start their sync pulse within {timeout:?}")]
    SyncTimeout {
        not_ready: Vec<u8>,
        timeout: Duration,
    },
    #[error("{field} must be in {min:?}..={max:?}, but {value:?} was given")]
    InvalidOption {
        field: &'static str,
        value: Duration,
        min: Duration,
        max: Duration,
    },
    #[error("the number of devices must be 1..=255, but {0} was given")]
    InvalidDeviceCount(usize),
    #[error("{expected} frames are needed (one per device), but tx has {tx} and rx has {rx}")]
    FrameCountMismatch {
        expected: usize,
        tx: usize,
        rx: usize,
    },
    #[error("the host clock cannot be read as a DC system time: {0}")]
    HostClock(#[from] autd3_rs_core::value::DcSysTimeError),
    #[error("the connection is closed")]
    Closed,
}
