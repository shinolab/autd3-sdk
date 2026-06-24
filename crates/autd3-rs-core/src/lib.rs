pub mod common;
pub mod error;
pub mod geometry;
pub mod link;
pub mod mirror;
pub mod params;
pub mod protocol;
pub mod value;

pub use common::units;
pub use common::{Angle, Freq, Length, Velocity};
pub use error::Error;
pub use geometry::{
    Autd3, Autd3Unity, Device, Geometry, Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3,
    offset, point,
};
pub use link::{
    ConstStateChecker, CycleOutcome, DeviceState, Interface, IntoLink, Link, LinkStats, LinkStatus,
    StateCheck,
};
pub use mirror::{FirmwareState, SilencerAxis, SilencerGuardState, SilencerViolation};
pub use protocol::{
    Cmd, DeviceErrorCode, MAX_IN_FLIGHT, PAYLOAD_BYTES, RX_FRAME_BYTES, RxFrame, Seq,
    TX_FRAME_BYTES, TxFrame, describe_device_error,
};
