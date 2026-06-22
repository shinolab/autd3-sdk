use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EtherCrabLinkError {
    #[error("ethercrab: {0}")]
    EtherCrab(#[from] ethercrab::error::Error),

    #[error("network io: {0}")]
    Io(#[from] std::io::Error),

    #[error("timed out waiting for subdevices to reach OP")]
    OpTimeout,

    #[error("DC clocks did not align within the timeout (max deviation {0:?})")]
    SyncTimeout(Duration),

    #[error("EtherCrabLink::open must be called from within a tokio runtime")]
    NoTokioRuntime,

    #[error("pcap: {0}")]
    Pcap(#[from] pcap::Error),

    #[error("No AUTD device found")]
    DeviceNotFound,

    #[error("subdevice {index} is not an AUTD device (name: {name:?})")]
    NotAutdDevice { index: usize, name: String },

    #[error("failed to split PDU storage")]
    PduStorage,

    #[error("the link is already closed")]
    Closed,
}
