mod control_point;
mod intensity;
mod phase;
mod pulse_width;
mod sampling_config;
mod transition_mode;

#[doc(hidden)]
pub use autd3_cpu_wire::value::Focus;
pub use autd3_cpu_wire::value::{GpioIn, GpioOut, LoopBehavior, SysTime};
pub use autd3_cpu_wire::{ModulationBank, PatternBank};
pub use control_point::{ControlPoint, ControlPoints};
pub use intensity::Intensity;
pub use phase::Phase;
pub use pulse_width::{PULSE_WIDTH_PERIOD, PulseWidth, PulseWidthError};
pub use sampling_config::{Nearest, SamplingConfig, SamplingConfigError};

#[doc(hidden)]
pub use sampling_config::is_integer;
pub use transition_mode::TransitionMode;
