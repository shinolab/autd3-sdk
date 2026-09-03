use std::convert::Infallible;
use std::io;

use autd3_rs_core::value::DcSysTime;
use autd3_rs_core::{
    CycleOutcome, DcClock, Geometry, Link, LinkStats, LinkStatus, RX_FRAME_BYTES, StateCheck,
    TX_FRAME_BYTES,
};

use crate::bus::{RawBus, RawSocket, interface_candidates};
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
            Some(name) => {
                let socket = RawSocket::open(name).map_err(|e| open_error(e, name))?;
                Master::open(socket, config)?
            }
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
        open_on_candidates(&interface_candidates()?, RawSocket::open, config)
    }
}

fn open_error(e: io::Error, interface: &str) -> EchocatError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        EchocatError::PermissionDenied {
            interface: Some(interface.to_owned()),
        }
    } else {
        EchocatError::Io(e)
    }
}

fn open_on_candidates<B: RawBus>(
    candidates: &[String],
    open: impl Fn(&str) -> io::Result<B>,
    config: MasterConfig,
) -> Result<Master<B>, EchocatError> {
    let mut denied = 0;
    for name in candidates {
        let socket = match open(name) {
            Ok(socket) => socket,
            Err(e) => {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    denied += 1;
                }
                tracing::debug!(interface = %name, "cannot open an interface: {e}");
                continue;
            }
        };
        let mut probe = Master::new(socket, config);
        match probe.enumerate() {
            Ok(devices) => {
                tracing::info!(interface = %name, devices, "found an EtherCAT bus");
                drop(probe);
                let socket = open(name).map_err(|e| open_error(e, name))?;
                return Master::open(socket, config);
            }
            Err(e) => tracing::debug!(interface = %name, "no EtherCAT bus here: {e}"),
        }
    }
    Err(if denied > 0 && denied == candidates.len() {
        EchocatError::PermissionDenied { interface: None }
    } else {
        EchocatError::NoInterfaceFound
    })
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::sim::EscSim;

    struct SilentBus;

    impl RawBus for SilentBus {
        fn send(&mut self, _frame: &[u8]) -> io::Result<()> {
            Ok(())
        }

        fn receive(&mut self, _buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
            std::thread::sleep(timeout);
            Ok(None)
        }

        fn mtu(&self) -> usize {
            1500
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn denied() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    fn silent_config() -> MasterConfig {
        MasterConfig {
            pdu_timeout: Duration::from_millis(5),
            ..MasterConfig::default()
        }
    }

    fn sim_config() -> MasterConfig {
        MasterConfig {
            cycle: Duration::from_millis(1),
            dc_static_sync_iterations: 32,
            dc_start_delay: Duration::from_millis(10),
            ..MasterConfig::default()
        }
    }

    #[test]
    fn every_candidate_denied_is_reported_as_a_permission_problem_not_a_missing_bus() {
        let e = open_on_candidates::<SilentBus>(
            &names(&["eth0", "eth1"]),
            |_| Err(denied()),
            silent_config(),
        )
        .err()
        .expect("no interface can be opened");

        assert!(
            matches!(e, EchocatError::PermissionDenied { interface: None }),
            "all candidates denied must not be reported as a missing bus, got {e}"
        );
    }

    #[test]
    fn a_candidate_that_fails_for_another_reason_keeps_no_interface_found() {
        let e = open_on_candidates::<SilentBus>(
            &names(&["eth0", "eth1"]),
            |name| {
                if name == "eth0" {
                    Err(denied())
                } else {
                    Err(io::Error::from(io::ErrorKind::NotFound))
                }
            },
            silent_config(),
        )
        .err()
        .expect("no interface can be opened");

        assert!(
            matches!(e, EchocatError::NoInterfaceFound),
            "a non-permission failure means the diagnosis is not permissions, got {e}"
        );
    }

    #[test]
    fn a_candidate_that_opens_without_a_bus_keeps_no_interface_found() {
        let e = open_on_candidates(
            &names(&["eth0", "eth1"]),
            |name| {
                if name == "eth0" {
                    Err(denied())
                } else {
                    Ok(SilentBus)
                }
            },
            silent_config(),
        )
        .err()
        .expect("the open interface has no EtherCAT bus");

        assert!(
            matches!(e, EchocatError::NoInterfaceFound),
            "an interface that opens but stays silent is a missing bus, got {e}"
        );
    }

    #[test]
    fn no_candidates_at_all_keeps_no_interface_found() {
        let e = open_on_candidates::<SilentBus>(&[], |_| Err(denied()), silent_config())
            .err()
            .expect("there is nothing to open");

        assert!(
            matches!(e, EchocatError::NoInterfaceFound),
            "an empty candidate list says nothing about permissions, got {e}"
        );
    }

    #[test]
    fn a_denied_candidate_does_not_stop_the_search() {
        let master = open_on_candidates(
            &names(&["eth0", "eth1"]),
            |name| {
                if name == "eth0" {
                    Err(denied())
                } else {
                    Ok(EscSim::nop(2, Duration::from_millis(1)))
                }
            },
            sim_config(),
        )
        .expect("the second candidate carries a bus");

        assert_eq!(2, master.num_devices());
    }

    #[test]
    fn a_named_interface_that_is_denied_names_the_interface() {
        let e = open_error(denied(), "enp5s0");

        assert!(
            matches!(&e, EchocatError::PermissionDenied { interface } if interface.as_deref() == Some("enp5s0")),
            "the interface the user asked for must survive into the error, got {e}"
        );
        assert!(e.to_string().contains("enp5s0"));
        #[cfg(target_os = "linux")]
        assert!(
            e.to_string().contains("setcap"),
            "the message must show how to grant the capability, got {e}"
        );
    }

    #[test]
    fn a_named_interface_that_fails_for_another_reason_stays_io() {
        let e = open_error(io::Error::from(io::ErrorKind::NotFound), "enp5s0");

        assert!(
            matches!(e, EchocatError::Io(_)),
            "a missing interface is not a permission problem, got {e}"
        );
    }
}
