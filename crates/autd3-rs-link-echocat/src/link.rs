use std::convert::Infallible;

use autd3_rs_core::value::DcSysTime;
use autd3_rs_core::{
    CycleOutcome, DcClock, Geometry, Link, LinkStats, LinkStatus, RX_FRAME_BYTES, StateCheck,
    TX_FRAME_BYTES,
};

use crate::bus::{RawSocket, interface_candidates};
use crate::error::EchocatError;
use crate::master::{BusState, Master, MasterConfig};
use crate::option::EchocatLinkOption;
use crate::timer::TimerResolutionGuard;

const TIMER_RESOLUTION_MS: u32 = 1;

pub struct StateChecker {
    state: BusState,
}

impl StateChecker {
    pub fn check(&mut self) -> Result<LinkStatus, Infallible> {
        Ok(LinkStatus::new(
            self.state.states(),
            self.state.recoveries(),
        ))
    }
}

impl StateCheck for StateChecker {
    type Error = Infallible;

    fn check(&mut self) -> Result<LinkStatus, Self::Error> {
        StateChecker::check(self)
    }
}

pub struct EchocatLink {
    master: Master<RawSocket>,
    state: BusState,
    stats: LinkStats,
    dc_clock: DcClock,
    rx_was_valid: bool,
    closed: bool,
    _timer_resolution: TimerResolutionGuard,
}

impl EchocatLink {
    pub fn open(option: &EchocatLinkOption) -> Result<Self, EchocatError> {
        option.validate()?;
        let timer_resolution = TimerResolutionGuard::new(TIMER_RESOLUTION_MS);
        let config = MasterConfig::from(option);
        let master = match option.iface.name() {
            Some(name) => Master::open(RawSocket::open(name)?, config)?,
            None => Self::open_on_any_interface(config)?,
        };
        let state = master.state();
        let link_stats = master.stats();
        Ok(Self {
            master,
            state,
            stats: link_stats,
            dc_clock: DcClock::new(),
            rx_was_valid: true,
            closed: false,
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
}

impl Drop for EchocatLink {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
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

    fn dc_clock(&self) -> Option<DcClock> {
        Some(self.dc_clock.clone())
    }

    fn wait_next_cycle(&mut self) {
        self.master.wait_next_cycle();
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        let report = self
            .master
            .cycle(tx.as_flattened(), rx.as_flattened_mut())?;

        if report.dc_system_time != 0 {
            let _ = self
                .dc_clock
                .observe(DcSysTime::from_nanos(report.dc_system_time));
        }
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

        Ok(if report.rx_valid {
            CycleOutcome::valid()
        } else {
            CycleOutcome::stale()
        })
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.master.close()
    }
}

impl autd3_rs_core::IntoLink for EchocatLinkOption {
    type Link = EchocatLink;

    fn into_link(
        self,
        geometry: &Geometry,
    ) -> Result<EchocatLink, autd3_rs_core::error::LinkError> {
        EchocatLink::open(&self)
            .map_err(|e| autd3_rs_core::error::LinkError::with_source(e.to_string(), e))
            .and_then(|link| {
                if link.num_devices() == geometry.num_devices() {
                    Ok(link)
                } else {
                    let e = EchocatError::DeviceCountMismatch {
                        expected: geometry.num_devices(),
                        received: link.num_devices(),
                    };
                    Err(autd3_rs_core::error::LinkError::with_source(
                        e.to_string(),
                        e,
                    ))
                }
            })
    }
}
