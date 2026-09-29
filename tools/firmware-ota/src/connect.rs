use std::time::Duration;

use anyhow::{Result, anyhow};
use clap::{Args, ValueEnum};

use autd3_rs::protocol::Seq;
use autd3_rs::{UdpBus, UdpError};
use autd3_rs_firmware_ota::{Dialect, EcatBus, EcatError, Exchange, Frame, Replies};

use crate::udp::UdpArgs;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum TransportKind {
    #[default]
    Auto,
    Udp,
    Ethercat,
}

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "EtherCAT (devices still running the v0.9.x firmware)")]
pub struct EcatArgs {
    #[arg(
        long,
        help = "Network interface of the EtherCAT devices (default: --interface on Linux / macOS, else auto-detect)"
    )]
    pub ecat_interface: Option<String>,
    #[arg(
        long,
        help = "EtherCAT SYNC0 period in microseconds (default: 2000); raise it if a long chain cannot finish a cycle in time"
    )]
    pub ecat_cycle_us: Option<u64>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ConnectArgs {
    #[arg(
        long,
        value_enum,
        default_value_t = TransportKind::Auto,
        help = "How to reach the devices: `auto` tries UDP (firmware v0.10.0 or newer) and falls back to EtherCAT (firmware v0.9.x)"
    )]
    pub transport: TransportKind,
    #[command(flatten)]
    pub udp: UdpArgs,
    #[command(flatten)]
    pub ecat: EcatArgs,
}

pub enum Connection<U = UdpBus, E = EcatBus> {
    Udp(U),
    EtherCat(E),
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError<U, E> {
    #[error(transparent)]
    Udp(U),
    #[error(transparent)]
    EtherCat(E),
}

impl Connection {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Udp(_) => "UDP (firmware v0.10.0 or newer)".to_owned(),
            Self::EtherCat(bus) => format!(
                "EtherCAT on {} (firmware v0.9.x; the CPU firmware can only be moved to the UDP firmware)",
                bus.interface()
            ),
        }
    }
}

impl<U, E> Connection<U, E> {
    #[must_use]
    pub fn is_ethercat(&self) -> bool {
        matches!(self, Self::EtherCat(_))
    }
}

impl<U: Exchange, E: Exchange> Exchange for Connection<U, E> {
    type Error = ConnectionError<U::Error, E::Error>;

    fn num_devices(&self) -> usize {
        match self {
            Self::Udp(bus) => bus.num_devices(),
            Self::EtherCat(bus) => bus.num_devices(),
        }
    }

    fn min_cpu_firmware_version(&self) -> (u8, u8, u8) {
        match self {
            Self::Udp(bus) => bus.min_cpu_firmware_version(),
            Self::EtherCat(bus) => bus.min_cpu_firmware_version(),
        }
    }

    fn dialect(&self) -> Dialect {
        match self {
            Self::Udp(bus) => bus.dialect(),
            Self::EtherCat(bus) => bus.dialect(),
        }
    }

    fn reset(&mut self, timeout: Duration) -> Result<bool, Self::Error> {
        match self {
            Self::Udp(bus) => bus.reset(timeout).map_err(ConnectionError::Udp),
            Self::EtherCat(bus) => bus.reset(timeout).map_err(ConnectionError::EtherCat),
        }
    }

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        timeout: Duration,
    ) -> Result<Replies, Self::Error> {
        match self {
            Self::Udp(bus) => bus
                .exchange(seq, frame, timeout)
                .map_err(ConnectionError::Udp),
            Self::EtherCat(bus) => bus
                .exchange(seq, frame, timeout)
                .map_err(ConnectionError::EtherCat),
        }
    }

    fn idle(&mut self, duration: Duration) -> Result<(), Self::Error> {
        match self {
            Self::Udp(bus) => bus.idle(duration).map_err(ConnectionError::Udp),
            Self::EtherCat(bus) => bus.idle(duration).map_err(ConnectionError::EtherCat),
        }
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        match self {
            Self::Udp(bus) => bus.close().map_err(ConnectionError::Udp),
            Self::EtherCat(bus) => bus.close().map_err(ConnectionError::EtherCat),
        }
    }
}

fn udp_found_nothing(e: &UdpError) -> bool {
    matches!(
        e,
        UdpError::NoInterfaceFound
            | UdpError::InterfaceNotFound(_)
            | UdpError::EnumerationTimeout { found: 0, .. }
    )
}

fn firewall_hint() -> String {
    let port = autd3_cpu_wire::udp::PORT;
    format!(
        "if the devices are connected and powered, the host firewall may be dropping their \
         replies: allow inbound UDP from fe80::/10 source port {port} on that interface \
         (e.g. `sudo ufw allow in on <interface> proto udp from fe80::/10 port {port}`; \
         on Windows, an inbound rule for UDP from remote port {port})"
    )
}

fn udp_error(e: UdpError) -> anyhow::Error {
    if udp_found_nothing(&e) {
        anyhow!("{e}; {}", firewall_hint())
    } else {
        e.into()
    }
}

fn connect<U, E>(
    kind: TransportKind,
    open_udp: impl FnOnce() -> Result<U, UdpError>,
    open_ecat: impl FnOnce() -> Result<E, EcatError>,
) -> Result<Connection<U, E>> {
    match kind {
        TransportKind::Udp => open_udp().map(Connection::Udp).map_err(udp_error),
        TransportKind::Ethercat => Ok(Connection::EtherCat(open_ecat()?)),
        TransportKind::Auto => {
            let udp = match open_udp() {
                Ok(bus) => return Ok(Connection::Udp(bus)),
                Err(e) if udp_found_nothing(&e) => e,
                Err(e) => return Err(udp_error(e)),
            };
            match open_ecat() {
                Ok(bus) => Ok(Connection::EtherCat(bus)),
                Err(ecat) if ecat.found_nothing() => Err(anyhow!(
                    "no AUTD3 device answered: over UDP (firmware v0.10.0 or newer), {udp}; \
                     over EtherCAT (firmware v0.9.x), {ecat}; {}",
                    firewall_hint()
                )),
                Err(ecat) => Err(anyhow::Error::new(ecat)
                    .context("AUTD3 devices answered over EtherCAT (firmware v0.9.x), but the bus could not be brought up")),
            }
        }
    }
}

impl ConnectArgs {
    pub fn open(&self, expected: Option<usize>) -> Result<Connection> {
        let kind = if self.udp.group.is_some() {
            TransportKind::Udp
        } else {
            self.transport
        };
        self.open_as(kind, expected)
    }

    pub fn open_udp(&self, expected: usize) -> Result<Connection> {
        self.open_as(TransportKind::Udp, Some(expected))
    }

    fn open_as(&self, kind: TransportKind, expected: Option<usize>) -> Result<Connection> {
        connect(
            kind,
            || self.udp.open(expected),
            || {
                EcatBus::open(
                    self.ecat_interface(),
                    self.ecat.ecat_cycle_us.map(Duration::from_micros),
                    expected,
                )
            },
        )
    }

    fn ecat_interface(&self) -> Option<&str> {
        self.ecat.ecat_interface.as_deref().or(if cfg!(windows) {
            None
        } else {
            self.udp.interface.as_deref()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn outcome(
        kind: TransportKind,
        udp: Result<(), UdpError>,
        ecat: Result<(), EcatError>,
    ) -> (Result<Connection<(), ()>>, bool) {
        let tried = Cell::new(false);
        let result = connect(
            kind,
            || udp,
            || {
                tried.set(true);
                ecat
            },
        );
        (result, tried.get())
    }

    #[test]
    fn auto_prefers_udp_and_leaves_ethercat_untouched() {
        let (result, tried) = outcome(TransportKind::Auto, Ok(()), Ok(()));
        assert!(matches!(result, Ok(Connection::Udp(()))));
        assert!(!tried);
    }

    #[test]
    fn auto_falls_back_to_ethercat_only_when_udp_finds_nothing() {
        for nothing in [
            UdpError::NoInterfaceFound,
            UdpError::InterfaceNotFound("eth0".to_owned()),
            UdpError::EnumerationTimeout {
                expected: 1,
                found: 0,
            },
        ] {
            let (result, tried) = outcome(TransportKind::Auto, Err(nothing), Ok(()));
            assert!(result.unwrap().is_ethercat());
            assert!(tried);
        }

        let (result, tried) = outcome(
            TransportKind::Auto,
            Err(UdpError::DeviceCountMismatch {
                expected: 2,
                found: 3,
            }),
            Ok(()),
        );
        assert!(result.is_err());
        assert!(!tried);
    }

    #[test]
    fn auto_reports_both_transports_when_neither_finds_a_device() {
        let (result, _) = outcome(
            TransportKind::Auto,
            Err(UdpError::NoInterfaceFound),
            Err(EcatError::NoInterfaceFound),
        );
        let message = result.err().unwrap().to_string();
        assert!(
            message.contains("UDP") && message.contains("EtherCAT"),
            "{message}"
        );
        assert!(
            message.contains("firewall") && message.contains("44336"),
            "{message}"
        );
    }

    #[test]
    fn an_ethercat_bus_that_answers_but_fails_is_not_hidden() {
        let (result, _) = outcome(
            TransportKind::Auto,
            Err(UdpError::NoInterfaceFound),
            Err(EcatError::DcSyncTimeout(std::time::Duration::from_secs(1))),
        );
        let e = result.err().unwrap();
        assert!(e.downcast_ref::<EcatError>().is_some());
    }

    #[test]
    fn only_a_udp_search_that_found_nothing_suggests_the_firewall() {
        let (result, _) = outcome(TransportKind::Udp, Err(UdpError::NoInterfaceFound), Ok(()));
        let message = result.err().unwrap().to_string();
        assert!(
            message.contains("firewall") && message.contains("44336"),
            "{message}"
        );

        let (result, _) = outcome(
            TransportKind::Udp,
            Err(UdpError::DeviceCountMismatch {
                expected: 2,
                found: 3,
            }),
            Ok(()),
        );
        assert!(!result.err().unwrap().to_string().contains("firewall"));
    }

    #[test]
    fn an_explicit_transport_never_tries_the_other() {
        let (result, tried) = outcome(TransportKind::Udp, Err(UdpError::NoInterfaceFound), Ok(()));
        assert!(result.is_err());
        assert!(!tried);
        let (result, _) = outcome(TransportKind::Ethercat, Ok(()), Ok(()));
        assert!(result.unwrap().is_ethercat());
    }
}
