mod bank;
mod control_point;
mod dc_sys_time;
mod emission;
mod focus;
mod gpio;
mod intensity;
mod loop_behavior;
mod phase;
mod pulse_width;
mod sampling_config;
mod transition_mode;

pub use bank::{ModulationBank, PatternBank};
pub use control_point::{ControlPoint, ControlPoints};
pub use dc_sys_time::{DcSysTime, DcSysTimeError};
pub use emission::Emission;
#[doc(hidden)]
pub use focus::Focus;
pub use gpio::GpioIn;
pub use intensity::Intensity;
pub use loop_behavior::LoopBehavior;
pub use phase::Phase;
pub use pulse_width::{PULSE_WIDTH_PERIOD, PulseWidth, PulseWidthError};
pub use sampling_config::{Nearest, SamplingConfig, SamplingConfigError};

#[doc(hidden)]
pub use sampling_config::is_integer;
pub use transition_mode::TransitionMode;
