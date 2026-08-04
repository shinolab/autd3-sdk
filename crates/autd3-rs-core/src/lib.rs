//! Core types shared across the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display)
//! sdk crates: geometry, units, values, the [`link::Link`] abstraction, and realtime tuning.
//!
//! Application code normally uses [`autd3-rs`](https://docs.rs/autd3-rs), which re-exports what it
//! needs from here. Depend on this crate directly only to write a `Link` or another sdk crate.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

pub mod common;
pub mod error;
pub mod geometry;
pub mod link;
pub mod params;
pub mod protocol;
pub mod rt;
pub mod value;

pub use rt::{
    CoreId, RtSchedulePolicy, RtThreadTuning, ThreadPriority, ThreadPriorityValue,
    apply_thread_tuning, default_rt_priority, step_below,
};

pub use nalgebra;

pub use common::units;
pub use common::{Angle, Freq, Length, Velocity};
pub use error::{EncodeError, LinkError};
pub use geometry::{
    Autd3, Device, Geometry, Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3, offset,
    point,
};
pub use link::{
    ConstStateChecker, CycleOutcome, DcClock, DcObservation, DeviceState, Interface, IntoLink,
    Link, LinkStats, LinkStatus, StateCheck,
};
pub use protocol::{
    Cmd, DeviceErrorCode, MAX_INFLIGHT, PAYLOAD_BYTES, RX_FRAME_BYTES, RxFrame, Seq,
    TX_FRAME_BYTES, TxFrame, describe_device_error,
};
