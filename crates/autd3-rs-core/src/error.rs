use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum EncodeError {
    #[error("focus coordinate {axis} = {value} out of range {min}..={max}")]
    FocusOutOfRange {
        axis: &'static str,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error(
        "transition mode `Later` only writes a bank without transitioning, so it cannot be encoded into a transition"
    )]
    TransitionLaterNotEncodable,
}

impl From<autd3_cpu_wire::value::FocusOutOfRange> for EncodeError {
    fn from(e: autd3_cpu_wire::value::FocusOutOfRange) -> Self {
        Self::FocusOutOfRange {
            axis: e.axis,
            value: e.value,
            min: e.min,
            max: e.max,
        }
    }
}
