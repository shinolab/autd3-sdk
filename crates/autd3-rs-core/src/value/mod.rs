mod control_point;
mod intensity;
mod pattern_intensity;
mod phase;
mod pulse_width;
mod sampling_config;

#[doc(hidden)]
pub use autd3_cpu_wire::value::Focus;
pub use autd3_cpu_wire::value::{GpioIn, GpioOut, LoopBehavior, SysTime, TransitionMode};
pub use autd3_cpu_wire::{ModulationBank, PatternBank};
pub use control_point::{ControlPoint, ControlPoints};
pub use intensity::Intensity;
pub use pattern_intensity::PatternIntensity;
pub use phase::Phase;
pub use pulse_width::{PULSE_WIDTH_PERIOD, PulseWidth, PulseWidthError};
pub use sampling_config::{SamplingConfig, SamplingConfigError};

#[doc(hidden)]
pub use sampling_config::is_integer;
