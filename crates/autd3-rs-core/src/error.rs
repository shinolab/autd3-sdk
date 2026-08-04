use thiserror::Error;

#[derive(Debug, Error)]
#[error("link error: {0}")]
pub struct LinkError(pub String);

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

    #[error("transition margin {0:?} is out of range (0..=4294967295 ns)")]
    TransitionMarginOutOfRange(core::time::Duration),

    #[error(
        "transition mode `Later` only writes a bank without transitioning, so it cannot be encoded into a transition"
    )]
    TransitionLaterNotEncodable,
}
