mod cycle;
pub(crate) mod group;
mod open;

use std::time::{Duration, Instant};

use ethercrab::Timeouts;
use ethercrab::subdevice_group::{HasDc, Op};
use tokio::runtime::Handle;

use crate::diagnostics::SharedCycleDiagnostics;
use crate::option::{EtherCrabLinkOption, EtherCrabLinkOptionFull};
use crate::osal::timer::TimerResolutionGuard;
use crate::transport::Transport;

use group::Groups;

impl autd3_rs_core::IntoLink for EtherCrabLinkOption {
    type Link = EtherCrabLink;

    async fn into_link(
        self,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<EtherCrabLink, autd3_rs_core::error::LinkError> {
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
    ) -> Result<EtherCrabLink, autd3_rs_core::error::LinkError> {
        Box::pin(EtherCrabLink::open(self))
            .await
            .map_err(|e| autd3_rs_core::error::LinkError::with_source(e.to_string(), e))
    }
}

pub struct EtherCrabLink {
    group: Option<Groups<Op, HasDc>>,
    addresses: Vec<u16>,
    transport: Transport,
    handle: Handle,
    next_at: Option<Instant>,
    cycle: Duration,
    shift: Duration,
    num_devices: usize,
    expected_wkc: u16,
    rx_was_valid: bool,
    timeouts: Timeouts,
    stats: autd3_rs_core::LinkStats,
    dc_clock: autd3_rs_core::DcClock,
    diagnostics: SharedCycleDiagnostics,
    _timer_resolution: TimerResolutionGuard,
}
