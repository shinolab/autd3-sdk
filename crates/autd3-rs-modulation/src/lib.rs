mod sampling;
mod sampling_mode;
mod sine;

pub use autd3_rs_core::value::Nearest;
pub use sampling::samples_per_period;
pub use sampling_mode::SamplingMode;
pub use sine::{SineOption, sine};
