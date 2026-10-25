use std::net::{Ipv6Addr, SocketAddrV6};
use std::time::Duration;

use clap::Args;

use autd3_rs::{Error, Interface, TransportOption, UdpError};

use crate::driver::{ACK_TIMEOUT, DEFAULT_HEARTBEAT, UdpLink};

const LOST_AFTER_HEARTBEATS: u32 = 10;

#[derive(Args)]
#[command(next_help_heading = "UDP (unset options keep the library defaults)")]
pub struct UdpArgs {
    #[arg(long, help = "Network interface name (default: auto-detect)")]
    pub interface: Option<String>,
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "44336",
        conflicts_with = "interface",
        help = "Connect to the simulator on this host only, listening on PORT (default 44336)"
    )]
    pub simulator: Option<u16>,
    #[arg(long, help = "Heartbeat interval while waiting, microseconds")]
    pub heartbeat_us: Option<u64>,
    #[arg(
        long,
        help = "How long opening the connection may take, enumeration included, milliseconds"
    )]
    pub open_timeout_ms: Option<u64>,
}

fn ms(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_millis)
}

fn us(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_micros)
}

impl UdpArgs {
    #[must_use]
    pub fn heartbeat(&self) -> Duration {
        us(self.heartbeat_us).unwrap_or(DEFAULT_HEARTBEAT)
    }

    #[must_use]
    pub fn option(&self) -> TransportOption {
        let default = TransportOption::default();
        TransportOption {
            iface: match self.simulator {
                Some(port) => Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
                None => self
                    .interface
                    .clone()
                    .map_or(Interface::Auto, Interface::Name),
            },
            lost_timeout: default
                .lost_timeout
                .max(self.heartbeat().saturating_mul(LOST_AFTER_HEARTBEATS)),
            open_timeout: ms(self.open_timeout_ms).unwrap_or(default.open_timeout),
            ack_timeout: ACK_TIMEOUT,
            sync_on_open: false,
            ..default
        }
    }

    pub fn open(&self, num_devices: usize) -> Result<UdpLink, OpenError> {
        Ok(UdpLink::open(
            &self.option(),
            num_devices,
            self.heartbeat(),
        )?)
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

#[derive(Debug, thiserror::Error)]
#[error("{0}{hint}", hint = hint(.0))]
pub struct OpenError(#[from] Error);

fn udp_cause(e: &Error) -> Option<&UdpError> {
    match e {
        Error::Network(cause) => Some(cause),
        _ => None,
    }
}

fn hint(e: &Error) -> String {
    if udp_cause(e).is_some_and(udp_found_nothing) {
        format!("; {}", firewall_hint())
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_firmware_emulator::udp::UdpEmulator;
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        udp: UdpArgs,
    }

    fn parse(args: &[&str]) -> UdpArgs {
        Cli::try_parse_from(std::iter::once("ota").chain(args.iter().copied()))
            .unwrap()
            .udp
    }

    #[test]
    fn no_udp_arguments_mean_the_library_defaults() {
        let args = parse(&[]);
        assert_eq!(
            args.option(),
            TransportOption {
                ack_timeout: ACK_TIMEOUT,
                sync_on_open: false,
                ..TransportOption::default()
            }
        );
        assert_eq!(args.heartbeat(), DEFAULT_HEARTBEAT);
    }

    #[test]
    fn every_udp_argument_reaches_the_option() {
        let args = parse(&[
            "--interface",
            "eth1",
            "--heartbeat-us",
            "20000",
            "--open-timeout-ms",
            "20000",
        ]);
        assert_eq!(
            args.option(),
            TransportOption {
                iface: Interface::Name("eth1".to_string()),
                lost_timeout: Duration::from_millis(200),
                open_timeout: Duration::from_secs(20),
                ack_timeout: ACK_TIMEOUT,
                sync_on_open: false,
                ..TransportOption::default()
            }
        );
        assert_eq!(args.heartbeat(), Duration::from_millis(20));
    }

    #[test]
    #[ignore = "depends on real time"]
    fn a_given_device_count_must_match_the_chain() {
        let emulator = UdpEmulator::spawn(3).unwrap();
        let option = TransportOption {
            iface: emulator.interface(),
            open_timeout: Duration::from_secs(6),
            sync_on_open: false,
            ..TransportOption::default()
        };
        let err = OpenError::from(UdpLink::open(&option, 2, DEFAULT_HEARTBEAT).err().unwrap());
        assert!(
            matches!(
                udp_cause(&err.0),
                Some(UdpError::DeviceCountMismatch {
                    expected: 2,
                    found: 3
                })
            ),
            "{err}"
        );
    }

    #[test]
    fn only_a_search_that_found_nothing_suggests_the_firewall() {
        for nothing in [
            UdpError::NoInterfaceFound,
            UdpError::InterfaceNotFound("eth0".to_owned()),
            UdpError::EnumerationTimeout {
                expected: 1,
                found: 0,
            },
        ] {
            let message = OpenError(nothing.into()).to_string();
            assert!(
                message.contains("firewall") && message.contains("44336"),
                "{message}"
            );
        }

        let message = OpenError(
            UdpError::DeviceCountMismatch {
                expected: 2,
                found: 3,
            }
            .into(),
        )
        .to_string();
        assert!(!message.contains("firewall"), "{message}");
    }

    #[test]
    fn the_simulator_flag_reaches_the_simulator_address_without_sending_a_geometry() {
        let simulator_at =
            |port| Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0));
        let args = parse(&["--simulator"]);
        assert_eq!(args.option().iface, simulator_at(autd3_rs::udp::PORT));
        let args = parse(&["--simulator", "50000"]);
        assert_eq!(args.option().iface, simulator_at(50000));
        assert!(Cli::try_parse_from(["ota", "--simulator", "--interface", "eth0"]).is_err());
    }
}
