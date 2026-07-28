use std::time::Duration;

use crate::reg::AlState;
use crate::wire::FrameError;

#[derive(Debug, thiserror::Error)]
pub enum EchocatError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Frame(#[from] FrameError),
    #[error("no response to an EtherCAT frame within {0:?}")]
    Timeout(Duration),
    #[error("working counter mismatch: expected {expected}, received {received}")]
    WorkingCounter { expected: u16, received: u16 },
    #[error("no EtherCAT subdevice responded on {0}")]
    NoSubDevices(String),
    #[error("no interface has an EtherCAT subdevice attached")]
    NoInterfaceFound,
    #[error(
        "subdevice {index} is not an AUTD3 device (vendor {vendor:#010x}, product {product:#010x})"
    )]
    ForeignSubDevice {
        index: usize,
        vendor: u32,
        product: u32,
    },
    #[error("subdevice count changed from {expected} to {received} during startup")]
    SubDeviceCountChanged { expected: usize, received: usize },
    #[error("geometry declares {expected} devices but {received} are attached to the bus")]
    DeviceCountMismatch { expected: usize, received: usize },
    #[error("SII read from subdevice {index} at word {word:#06x} timed out")]
    SiiTimeout { index: usize, word: u16 },
    #[error(
        "subdevice {index} refused the transition to {target:?}: AL status {status:?}, code {code:#06x}"
    )]
    AlTransition {
        index: usize,
        target: AlState,
        status: Option<AlState>,
        code: u16,
    },
    #[error("the bus did not reach {target:?} within {timeout:?}")]
    AlTimeout { target: AlState, timeout: Duration },
    #[error("distributed clocks did not settle within {0:?}")]
    DcSyncTimeout(Duration),
    #[error(
        "propagation delay to subdevice {index} measured as {delay_ns} ns, which is far too long for an EtherCAT segment"
    )]
    ImplausiblePropagationDelay { index: usize, delay_ns: u32 },
    #[error("the link is closed")]
    Closed,
}
