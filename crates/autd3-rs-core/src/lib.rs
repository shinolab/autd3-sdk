pub mod bus;
pub mod common;
pub mod fpga_bitstream;
pub mod geometry;
pub mod params;
pub mod protocol;
pub mod value;

pub use nalgebra;

pub use bus::{DeviceState, DeviceStatus, Interface};
pub use common::units;
pub use common::{Angle, Freq, Length, Velocity};
#[cfg(feature = "serde")]
pub use geometry::LayoutError;
pub use geometry::{
    Autd3, Device, Geometry, Point3, Quaternion, TransducerMask, TransducerMaskError,
    UnitQuaternion, UnitVector3, Vector3, offset, point,
};
pub use protocol::{
    Cmd, DeviceErrorCode, FRAME_BYTES_MAX, MAX_DEVICES, MAX_INFLIGHT, PAYLOAD_BYTES,
    REPLY_DATA_BYTES_MAX, describe_device_error,
};
