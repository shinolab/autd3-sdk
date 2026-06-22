mod sampling;
mod sampling_mode;
mod sine;

pub use sampling::samples_per_period;
pub use sampling_mode::{Nearest, SamplingMode};
pub use sine::{SineOption, sine};
