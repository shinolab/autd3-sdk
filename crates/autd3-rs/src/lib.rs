pub mod commands;
pub mod error;

mod client;
mod datagram;
mod firmware_version;
mod fpga_state;
mod response;
mod telemetry;
pub mod udp;

mod sealed {
    pub trait Sealed {}
}

#[cfg(test)]
mod test_utils;

pub use autd3_rs_core::{bus, common, geometry, nalgebra, params, protocol, rt, units, value};
pub use error::{Error, NetworkCause, PayloadError};

#[cfg(feature = "serde")]
pub use autd3_rs_core::LayoutError;
pub use autd3_rs_core::{
    Angle, Autd3, BusStats, Device, DeviceClock, DeviceState, DeviceStatus, EncodeError, Freq,
    Geometry, Interface, Length, MAX_INFLIGHT, Point3, Quaternion, UnitQuaternion, UnitVector3,
    Vector3, Velocity, offset, point,
};
pub use client::{
    Client, ClientConfig, Controller, Driver, MAX_DEVICES, ResponseFuture, StreamFuture,
};
pub use datagram::{Datagram, Frame, FrameIter, Frames};
pub use firmware_version::{FirmwareVersion, Version};
pub use fpga_state::FpgaState;
pub use response::Response;
pub use telemetry::{Telemetry, TelemetryCounters};
pub use udp::{Reply, Sent, StateChecker, TransportOption, UdpBus, UdpError, Unsent};
