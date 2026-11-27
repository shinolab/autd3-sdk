pub mod commands;
pub mod error;

mod client;
mod controller;
mod datagram;
mod device_status;
mod firmware_version;
mod fpga_state;
mod response;
mod sync_ps;
mod telemetry;
pub mod udp;

mod sealed {
    pub trait Sealed {}
}

#[cfg(test)]
mod test_utils;

pub use autd3_rs_core::{bus, common, geometry, nalgebra, params, protocol, units, value};
pub use error::{Error, PayloadError};

#[doc(hidden)]
pub use autd3_cpu_wire::payload::SyncPsStats;
pub use autd3_rs_core::{
    Angle, Autd3, Device, DeviceState, Freq, Geometry, Interface, Length, MAX_DEVICES,
    MAX_INFLIGHT, Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3, Velocity, offset,
    point,
};
pub use client::Client;
pub use controller::{Controller, ExecAt, InflightFrames};
pub use datagram::{Datagram, Frame, FrameIter, Frames};
pub use device_status::DeviceStatus;
pub use firmware_version::{FirmwareVersion, Version};
pub use fpga_state::FpgaState;
pub use response::Response;
#[doc(hidden)]
pub use sync_ps::TdcCapture;
pub use telemetry::Telemetry;
pub use udp::{DeviceReply, TransportOption, UdpError};
