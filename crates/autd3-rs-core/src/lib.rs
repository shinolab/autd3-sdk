pub mod common;
pub mod error;
pub mod geometry;
pub mod link;
pub mod params;
pub mod protocol;
pub mod rt;
pub mod value;

pub use rt::RtSchedulePolicy;

pub use common::units;
pub use common::{Angle, Freq, Length, Velocity};
pub use error::{EncodeError, LinkError};
pub use geometry::{
    Autd3, Device, Geometry, Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3, offset,
    point,
};
pub use link::{
    ConstStateChecker, CycleOutcome, DeviceState, Interface, IntoLink, Link, LinkStats, LinkStatus,
    StateCheck,
};
pub use protocol::{
    Cmd, DeviceErrorCode, MAX_INFLIGHT, PAYLOAD_BYTES, RX_FRAME_BYTES, RxFrame, Seq,
    TX_FRAME_BYTES, TxFrame, describe_device_error,
};
