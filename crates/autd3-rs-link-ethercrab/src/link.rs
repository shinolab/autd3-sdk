mod cycle;
mod open;

use std::time::Instant;

use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use ethercrab::subdevice_group::{HasDc, NoDc, Op, PreOpPdi, SafeOp};
use ethercrab::{DefaultLock, SubDeviceGroup};
use tokio::runtime::Handle;

use crate::diagnostics::SharedCycleDiagnostics;
use crate::option::{EtherCrabLinkOption, EtherCrabLinkOptionFull};
use crate::transport::Transport;

pub(crate) const MAX_SUBDEVICES: usize = 32;
pub(crate) const PDI_LEN: usize = (TX_FRAME_BYTES + RX_FRAME_BYTES) * MAX_SUBDEVICES;
const OP_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const OP_WKC_STABLE_CYCLES: u32 = 5;
const TIMER_RESOLUTION_MS: u32 = 1;
const GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const OP_WARMUP_CYCLES: u32 = 200;

pub(crate) const SUBDEVICE_NAME: &str = "AUTD";

pub(crate) type PreOpPdiGroup =
    SubDeviceGroup<MAX_SUBDEVICES, PDI_LEN, DefaultLock, PreOpPdi, NoDc>;
type SafeOpGroup = SubDeviceGroup<MAX_SUBDEVICES, PDI_LEN, DefaultLock, SafeOp, HasDc>;
type OpGroup = SubDeviceGroup<MAX_SUBDEVICES, PDI_LEN, DefaultLock, Op, HasDc>;

impl autd3_rs_core::IntoLink for EtherCrabLinkOption {
    type Link = EtherCrabLink;

    async fn into_link(self) -> Result<EtherCrabLink, autd3_rs_core::Error> {
        EtherCrabLinkOptionFull::from(self).into_link().await
    }
}

impl autd3_rs_core::IntoLink for EtherCrabLinkOptionFull {
    type Link = EtherCrabLink;

    async fn into_link(self) -> Result<EtherCrabLink, autd3_rs_core::Error> {
        Box::pin(EtherCrabLink::open(self))
            .await
            .map_err(|e| autd3_rs_core::Error::Link(e.to_string()))
    }
}

pub struct EtherCrabLink {
    group: Option<OpGroup>,
    addresses: Vec<u16>,
    transport: Transport,
    handle: Handle,
    next_at: Option<Instant>,
    num_devices: usize,
    expected_wkc: u16,
    rx_was_valid: bool,
    stats: autd3_rs_core::LinkStats,
    diagnostics: SharedCycleDiagnostics,
    _timer_resolution: crate::timer::TimerResolutionGuard,
}
