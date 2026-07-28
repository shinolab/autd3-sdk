use std::convert::Infallible;

use autd3_rs_core::{
    CycleOutcome, Geometry, Link, LinkStats, LinkStatus, RX_FRAME_BYTES, StateCheck, TX_FRAME_BYTES,
};

use crate::bus::{RawSocket, interface_candidates};
use crate::error::EchocatError;
use crate::master::{BusState, Master, MasterConfig};
use crate::option::EchocatLinkOption;
use crate::timer::TimerResolutionGuard;

const RECOVERY_BACKOFF_CYCLES: u32 = 8;
const TIMER_RESOLUTION_MS: u32 = 1;

pub struct StateChecker {
    state: BusState,
}

impl StateCheck for StateChecker {
    type Error = Infallible;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        let status = LinkStatus {
            devices: self.state.states(),
            recoveries: self.state.recoveries(),
        };
        std::future::ready(Ok(status))
    }
}

pub struct EchocatLink {
    master: Master<RawSocket>,
    state: BusState,
    stats: LinkStats,
    rx_was_valid: bool,
    recovery_backoff: u32,
    _timer_resolution: TimerResolutionGuard,
}

impl EchocatLink {
    pub fn open(option: &EchocatLinkOption) -> Result<Self, EchocatError> {
        let timer_resolution = TimerResolutionGuard::new(TIMER_RESOLUTION_MS);
        let config = MasterConfig::from(option);
        let master = match option.iface.name() {
            Some(name) => Master::open(RawSocket::open(name)?, config)?,
            None => Self::open_on_any_interface(config)?,
        };
        let state = master.state();
        Ok(Self {
            master,
            state,
            stats: LinkStats::default(),
            rx_was_valid: true,
            recovery_backoff: 0,
            _timer_resolution: timer_resolution,
        })
    }

    fn open_on_any_interface(config: MasterConfig) -> Result<Master<RawSocket>, EchocatError> {
        for name in interface_candidates()? {
            let Ok(socket) = RawSocket::open(&name) else {
                continue;
            };
            let mut probe = Master::new(socket, config);
            match probe.enumerate() {
                Ok(devices) => {
                    tracing::info!(interface = %name, devices, "found an EtherCAT bus");
                    let socket = RawSocket::open(&name)?;
                    return Master::open(socket, config);
                }
                Err(e) => tracing::debug!(interface = %name, "no EtherCAT bus here: {e}"),
            }
        }
        Err(EchocatError::NoInterfaceFound)
    }

    pub fn close(&mut self) -> Result<(), EchocatError> {
        self.master.close()
    }
}

impl Drop for EchocatLink {
    fn drop(&mut self) {
        if let Err(e) = self.master.close() {
            tracing::warn!("failed to return the bus to INIT: {e}");
        }
    }
}

impl Link for EchocatLink {
    type Error = EchocatError;
    type Checker = StateChecker;

    fn num_devices(&self) -> usize {
        self.master.num_devices()
    }

    fn stats(&self) -> LinkStats {
        self.stats.clone()
    }

    fn state_checker(&self) -> StateChecker {
        StateChecker {
            state: self.state.clone(),
        }
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        let report = self
            .master
            .cycle(tx.as_flattened(), rx.as_flattened_mut())?;

        if !report.rx_valid {
            self.stats.record_stale_cycle();
            if report.dc_system_time == 0 {
                self.stats.record_lost_cycle();
            }
        }
        if report.rx_valid != self.rx_was_valid {
            if report.rx_valid {
                tracing::info!("bus recovered, rx valid again");
            } else {
                tracing::warn!(
                    al_status = report.al_status,
                    dc_system_time = report.dc_system_time,
                    "stale cycle: devices did not process this cycle",
                );
            }
            self.rx_was_valid = report.rx_valid;
        }

        if self.recovery_backoff > 0 {
            self.recovery_backoff -= 1;
        } else if self.master.is_op() && !self.state.all_op() {
            if let Err(e) = self.master.recover_op() {
                tracing::warn!("bus recovery did not go through this cycle: {e}");
            }
            self.recovery_backoff = RECOVERY_BACKOFF_CYCLES;
        }

        Ok(CycleOutcome {
            rx_valid: report.rx_valid,
        })
    }
}

impl autd3_rs_core::IntoLink for EchocatLinkOption {
    type Link = EchocatLink;

    async fn into_link(
        self,
        geometry: &Geometry,
    ) -> Result<EchocatLink, autd3_rs_core::error::LinkError> {
        let link =
            EchocatLink::open(&self).map_err(|e| autd3_rs_core::error::LinkError(e.to_string()))?;
        if link.num_devices() != geometry.num_devices() {
            return Err(autd3_rs_core::error::LinkError(
                EchocatError::DeviceCountMismatch {
                    expected: geometry.num_devices(),
                    received: link.num_devices(),
                }
                .to_string(),
            ));
        }
        Ok(link)
    }
}
