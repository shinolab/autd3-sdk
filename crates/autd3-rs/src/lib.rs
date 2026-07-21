pub mod commands;
pub mod error;
pub mod firmware_version;
pub mod fpga_state;
pub mod telemetry;
pub mod tuning;

mod client;
mod datagram;
mod response;

pub use autd3_rs_core::{common, geometry, link, mirror, params, protocol, units, value};
pub use error::{Error, PayloadError};

pub use autd3_rs_core::value::{ControlPoint, ControlPoints, PulseWidth, PulseWidthError};

pub use autd3_rs_core::{
    Angle, Autd3, ConstStateChecker, CycleOutcome, Device, DeviceState, EncodeError, Freq,
    Geometry, Interface, IntoLink, Length, Link, LinkError, LinkStats, LinkStatus, MAX_IN_FLIGHT,
    Point3, Quaternion, RtSchedulePolicy, StateCheck, UnitQuaternion, UnitVector3, Vector3,
    Velocity, offset, point,
};
pub use client::{Client, ClientConfig, ResponseFuture};
pub use core_affinity::CoreId;
pub use datagram::{Datagram, DatagramBuilder, Frame, Frames};
pub use firmware_version::{FirmwareVersion, Version};
pub use fpga_state::FpgaState;
pub use response::Response;
pub use telemetry::Telemetry;
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};
pub use tuning::PerfTuning;
