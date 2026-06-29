pub mod command;
pub mod firmware_version;
pub mod fpga_state;
pub mod operation;
pub mod stm;
pub mod tuning;

mod client;
mod datagram;
mod response;

pub use autd3_rs_core::{common, error, geometry, link, mirror, params, protocol, units, value};

pub use autd3_rs_core::value::{
    ControlPoint, ControlPoints, PULSE_WIDTH_PERIOD, PulseWidth, PulseWidthError,
};

pub use autd3_rs_core::{
    Angle, Autd3, Autd3Unity, Cmd, ConstStateChecker, CycleOutcome, Device, DeviceState, Error,
    FirmwareState, Freq, Geometry, Interface, IntoLink, Length, Link, LinkStats, LinkStatus,
    MAX_IN_FLIGHT, PAYLOAD_BYTES, Point3, Quaternion, RX_FRAME_BYTES, RxFrame, Seq, SilencerAxis,
    SilencerGuardState, SilencerViolation, StateCheck, TX_FRAME_BYTES, TxFrame, UnitQuaternion,
    UnitVector3, Vector3, Velocity, offset, point,
};
pub use client::{Client, ClientConfig, MAX_DEVICES, ResponseFuture};
pub use command::{BoxedCommand, Command, Modulation, Pattern};
pub use core_affinity::CoreId;
pub use datagram::{Datagram, DatagramBuilder, Datagrams, Frame};
pub use firmware_version::FirmwareVersion;
pub use fpga_state::FpgaState;
pub use operation::{
    ChangeModulationBank, ChangePatternBank, Clear, ConfigFociStm, ConfigModulation, ConfigPattern,
    Distribution, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate, ForceFan, GpioOut, Nop,
    Operation, PWE_TABLE_SIZE, PatternCompression, SetGpioOut, SetOutputMask, SetPhaseCorrection,
    SetPulseWidthTable, SetSilencer, SilencerConfig, WriteFociBuffer, WriteModulationBuffer,
    WritePatternBuffer, WritePatternCompressed, XorHashCmd,
};
pub use response::Response;
pub use stm::{
    FociStm, FociStmOption, PatternStm, PatternStmMode, PatternStmOption, StmConfig, circle, line,
};
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};
pub use tuning::PerfTuning;
