pub mod bus;
pub mod common;
pub mod error;
pub mod geometry;
pub mod params;
pub mod protocol;
pub mod rt;
pub mod value;

pub use chrono;
pub use nalgebra;

pub use bus::{BusStats, ClockObservation, DeviceClock, DeviceState, DeviceStatus, Interface};
pub use common::units;
pub use common::{Angle, Freq, Length, Velocity};
pub use error::EncodeError;
#[cfg(feature = "serde")]
pub use geometry::LayoutError;
pub use geometry::{
    Autd3, Device, Geometry, Point3, Quaternion, TransducerGroups, TransducerMask,
    TransducerMaskError, UnitQuaternion, UnitVector3, Vector3, offset, point,
};
pub use protocol::{
    Cmd, DeviceErrorCode, FRAME_BYTES_MAX, MAX_INFLIGHT, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX, Seq,
    TxFrame, describe_device_error,
};
