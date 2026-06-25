mod cycle;
mod open;

use std::future::Future;
use std::time::{Duration, Instant};

use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use ethercrab::subdevice_group::{HasDc, HasPdi, NoDc, Op, PreOp};
use ethercrab::{DefaultLock, MainDevice, SubDeviceGroup};
use futures_util::future::join_all;
use tokio::runtime::Handle;

use crate::diagnostics::SharedCycleDiagnostics;
use crate::option::{EtherCrabLinkOption, EtherCrabLinkOptionFull};
use crate::transport::Transport;

pub(crate) const MAX_SUBDEVICES: usize = 32;
// Splitting devices into groups of two keeps each EtherCAT frame below the
// Ethernet PDU capacity: a single combined frame
// would exceed the maximum frame size with three or more devices.
pub(crate) const GROUP_SUBDEVICES: usize = 2;
pub(crate) const SUB_GROUP_PDI_LEN: usize = (TX_FRAME_BYTES + RX_FRAME_BYTES) * GROUP_SUBDEVICES;
pub(crate) const MAX_GROUPS: usize = MAX_SUBDEVICES / GROUP_SUBDEVICES;
pub(crate) const DETECT_PDI_LEN: usize = (TX_FRAME_BYTES + RX_FRAME_BYTES) * MAX_SUBDEVICES;
const OP_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const OP_WKC_STABLE_CYCLES: u32 = 5;
const TIMER_RESOLUTION_MS: u32 = 1;
const GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const OP_WARMUP_CYCLES: u32 = 200;

pub(crate) const SUBDEVICE_NAME: &str = "AUTD";

pub(crate) type SubGroup<S, DC> =
    SubDeviceGroup<GROUP_SUBDEVICES, SUB_GROUP_PDI_LEN, DefaultLock, S, DC>;

pub(crate) struct Groups<S = PreOp, DC = NoDc> {
    pub(crate) groups: Vec<SubGroup<S, DC>>,
}

impl<S, DC> Groups<S, DC> {
    pub(crate) fn num_devices(&self) -> usize {
        self.groups.iter().map(SubGroup::len).sum()
    }

    pub(crate) async fn transform<S2, DC2, E, Fut>(
        self,
        f: impl Fn(SubGroup<S, DC>) -> Fut,
    ) -> Result<Groups<S2, DC2>, E>
    where
        Fut: Future<Output = Result<SubGroup<S2, DC2>, E>>,
    {
        let mut groups = Vec::with_capacity(self.groups.len());
        for result in join_all(self.groups.into_iter().map(f)).await {
            groups.push(result?);
        }
        Ok(Groups { groups })
    }
}

pub(crate) struct AggregatedResponse {
    pub(crate) working_counter: u16,
    pub(crate) all_op: bool,
    pub(crate) next_cycle_wait: Duration,
    pub(crate) dc_system_time: u64,
    pub(crate) cycle_start_offset: Duration,
}

impl<S: HasPdi> Groups<S, HasDc> {
    pub(crate) async fn tx_rx_dc(
        &self,
        maindevice: &MainDevice<'_>,
    ) -> Result<AggregatedResponse, ethercrab::error::Error> {
        let responses = join_all(self.groups.iter().map(|g| g.tx_rx_dc(maindevice))).await;
        let mut agg = AggregatedResponse {
            working_counter: 0,
            all_op: true,
            next_cycle_wait: Duration::ZERO,
            dc_system_time: 0,
            cycle_start_offset: Duration::ZERO,
        };
        let mut first = true;
        for response in responses {
            let response = response?;
            agg.working_counter = agg.working_counter.saturating_add(response.working_counter);
            agg.all_op &= response.all_op();
            agg.next_cycle_wait = agg.next_cycle_wait.max(response.extra.next_cycle_wait);
            if first {
                agg.dc_system_time = response.extra.dc_system_time;
                agg.cycle_start_offset = response.extra.cycle_start_offset;
                first = false;
            }
        }
        Ok(agg)
    }
}

impl autd3_rs_core::IntoLink for EtherCrabLinkOption {
    type Link = EtherCrabLink;

    async fn into_link(
        self,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<EtherCrabLink, autd3_rs_core::Error> {
        EtherCrabLinkOptionFull::from(self)
            .into_link(geometry)
            .await
    }
}

impl autd3_rs_core::IntoLink for EtherCrabLinkOptionFull {
    type Link = EtherCrabLink;

    async fn into_link(
        self,
        _geometry: &autd3_rs_core::Geometry,
    ) -> Result<EtherCrabLink, autd3_rs_core::Error> {
        Box::pin(EtherCrabLink::open(self))
            .await
            .map_err(|e| autd3_rs_core::Error::Link(e.to_string()))
    }
}

pub struct EtherCrabLink {
    group: Option<Groups<Op, HasDc>>,
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
