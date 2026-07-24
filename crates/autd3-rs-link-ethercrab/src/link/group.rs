use std::future::Future;
use std::time::Duration;

use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use ethercrab::error::TimeoutError;
use ethercrab::subdevice_group::{HasDc, HasPdi, NoDc, PreOp};
use ethercrab::{DefaultLock, MainDevice, SubDeviceGroup};
use futures_util::future::join_all;

use crate::join::join_bounded;
use crate::timeout::with_timeout;

pub(crate) const MAX_SUBDEVICES: usize = 32;
// Splitting devices into groups of two keeps each EtherCAT frame below the
// Ethernet PDU capacity: a single combined frame
// would exceed the maximum frame size with three or more devices.
pub(crate) const GROUP_SUBDEVICES: usize = 2;
pub(crate) const SUB_GROUP_PDI_LEN: usize = (TX_FRAME_BYTES + RX_FRAME_BYTES) * GROUP_SUBDEVICES;
pub(crate) const MAX_GROUPS: usize = MAX_SUBDEVICES / GROUP_SUBDEVICES;
pub(crate) const DETECT_PDI_LEN: usize = (TX_FRAME_BYTES + RX_FRAME_BYTES) * MAX_SUBDEVICES;

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
        let groups = join_all(self.groups.into_iter().map(f))
            .await
            .into_iter()
            .collect::<Result<Vec<_>, E>>()?;
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
        pdu_timeout: Duration,
    ) -> Result<AggregatedResponse, ethercrab::error::Error> {
        with_timeout(
            pdu_timeout,
            TimeoutError::Pdu,
            self.tx_rx_dc_inner(maindevice),
        )
        .await
    }

    async fn tx_rx_dc_inner(
        &self,
        maindevice: &MainDevice<'_>,
    ) -> Result<AggregatedResponse, ethercrab::error::Error> {
        let responses =
            join_bounded::<MAX_GROUPS, _>(self.groups.iter().map(|g| g.tx_rx_dc(maindevice))).await;
        let mut agg = AggregatedResponse {
            working_counter: 0,
            all_op: true,
            next_cycle_wait: Duration::ZERO,
            dc_system_time: 0,
            cycle_start_offset: Duration::ZERO,
        };
        let mut first = true;
        for response in responses.into_iter().flatten() {
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
