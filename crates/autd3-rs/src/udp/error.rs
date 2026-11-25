use std::net::SocketAddrV6;

use autd3_cpu_wire::udp::Kind;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UdpError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("no interface has an AUTD3 device answering ReadUnitInfo")]
    NoInterfaceFound,
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
    Rejected {
        kind: Kind,
        unit_id: usize,
        status: u8,
    },
    #[error("unit {unit_id} did not answer {kind:?}")]
    NoResponse { kind: Kind, unit_id: usize },
    #[error(
        "{replies} unassigned devices answered one Discover; a device failed to close its downstream port"
    )]
    DownstreamNotClosed { replies: usize },
    #[error("geometry declares {expected} devices but {found} are attached")]
    DeviceCountMismatch { expected: usize, found: usize },
    #[error("an unexpected unit (id {unit_id:#04x}) at {addr} answered the cross-check")]
    UnexpectedUnit { unit_id: usize, addr: SocketAddrV6 },
    #[error("unit {0} did not answer the cross-check")]
    MissingUnit(usize),
    #[error("the number of devices must be 1..=128, but {0} was given")]
    InvalidDeviceCount(usize),
    #[error("the frame could not be sent to device {device}: {source}")]
    Unsent {
        device: usize,
        source: std::io::Error,
    },
}
