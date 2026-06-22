use std::time::Duration;

use thiserror::Error;

use crate::state::AlState;

#[derive(Debug, Error)]
pub enum SoemLinkError {
    #[error("no socket connection on interface {0:?} (root / CAP_NET_RAW required?)")]
    NoSocketConnection(String),

    #[error("invalid interface name: {0:?}")]
    InvalidInterfaceName(String),

    #[error("No AUTD device found")]
    DeviceNotFound,

    #[error("subdevice {index} is not an AUTD device (name: {name:?})")]
    NotAutdDevice { index: usize, name: String },

    #[error("devices did not reach {expected} (actual: {actual})")]
    StateTransitionFailed { expected: AlState, actual: AlState },

    #[error("timed out waiting for subdevices to reach OP")]
    OpTimeout,

    #[error("DC clocks did not align within the timeout (max deviation {0:?})")]
    SyncTimeout(Duration),

    #[error("the link is already closed")]
    Closed,
}
