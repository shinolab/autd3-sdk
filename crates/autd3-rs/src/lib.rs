pub mod commands;
pub mod error;
#[cfg(feature = "legacy")]
pub mod legacy;
pub mod mirror;

mod client;
mod datagram;
mod firmware_version;
mod fpga_state;
mod response;
mod telemetry;
mod tuning;

mod sealed {
    pub trait Sealed {}
}

#[cfg(test)]
mod test_utils;

pub use autd3_rs_core::{common, geometry, link, nalgebra, params, protocol, rt, units, value};
pub use error::{Error, PayloadError};

#[cfg(feature = "serde")]
pub use autd3_rs_core::LayoutError;
pub use autd3_rs_core::{
    Angle, Autd3, ConstStateChecker, CoreId, CycleOutcome, DcClock, DcObservation, Device,
    DeviceState, EncodeError, Freq, Geometry, Interface, IntoLink, Length, Link, LinkError,
    LinkStats, LinkStatus, MAX_INFLIGHT, Point3, Quaternion, RtPriority, RtSchedulePolicy,
    StateCheck, UnitQuaternion, UnitVector3, Vector3, Velocity, offset, point,
};
pub use client::{Client, ClientConfig, MAX_DEVICES, ResponseFuture};
pub use datagram::{Datagram, DatagramBuilder, Frame, FrameIter, Frames};
pub use firmware_version::{FirmwareVersion, Version};
pub use fpga_state::FpgaState;
pub use response::Response;
pub use telemetry::Telemetry;
pub use tuning::PerfTuning;
