use thiserror::Error;

type BoxError = Box<dyn core::error::Error + Send + Sync>;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct LinkError {
    message: String,
    #[source]
    source: Option<BoxError>,
}

impl LinkError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(message: impl Into<String>, source: impl Into<BoxError>) -> Self {
        Self {
            message: message.into(),
            source: Some(source.into()),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

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
