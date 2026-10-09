use std::time::Duration;

use anyhow::{Result, anyhow};
use clap::Args;

use autd3_rs::{Interface, MAX_DEVICES, TransportOption, UdpBus, UdpError};

const LOST_AFTER_HEARTBEATS: u32 = 10;

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "UDP (unset options keep the library defaults)")]
pub struct UdpArgs {
    #[arg(long, help = "Network interface name (default: auto-detect)")]
    pub interface: Option<String>,
    #[arg(
        long,
        conflicts_with = "interface",
        help = "Connect to the simulator on this host only (default: the simulator if it runs, else the devices)"
    )]
    pub simulator: bool,
    #[arg(long, help = "Heartbeat interval while waiting, microseconds")]
    pub heartbeat_us: Option<u64>,
    #[arg(
        long,
        help = "How long a management request waits for its reply, milliseconds"
    )]
    pub response_timeout_ms: Option<u64>,
    #[arg(
        long,
        help = "How long the enumeration waits for the chain to reach the device count, milliseconds"
    )]
    pub enumeration_timeout_ms: Option<u64>,
}

fn ms(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_millis)
}

fn us(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_micros)
}

impl UdpArgs {
    #[must_use]
    pub fn option(&self) -> TransportOption {
        let default = TransportOption::default();
        let heartbeat = us(self.heartbeat_us).or(default.heartbeat);
        TransportOption {
            iface: if self.simulator {
                Interface::Simulator
            } else {
                self.interface.clone().into()
            },
            heartbeat,
            lost_timeout: default.lost_timeout.max(
                heartbeat
                    .unwrap_or_default()
                    .saturating_mul(LOST_AFTER_HEARTBEATS),
            ),
            response_timeout: ms(self.response_timeout_ms).unwrap_or(default.response_timeout),
            enumeration_timeout: ms(self.enumeration_timeout_ms)
                .unwrap_or(default.enumeration_timeout),
            ..default
        }
    }

    pub fn open(&self, expected: Option<usize>) -> Result<UdpBus> {
        self.enumerate(expected).map_err(udp_error)
    }

    fn enumerate(&self, expected: Option<usize>) -> Result<UdpBus, UdpError> {
        let option = self.option();
        if let Some(devices) = expected {
            return UdpBus::open_unsynchronized(&option, devices);
        }
        let mut devices = 1;
        loop {
            match UdpBus::open_unsynchronized(&option, devices) {
                Err(UdpError::DeviceCountMismatch { found, .. })
                    if found > devices && found <= MAX_DEVICES =>
                {
                    devices = found;
                }
                other => return other,
            }
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

#[cfg(test)]
mod tests {
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
        assert_eq!(parse(&[]).option(), TransportOption::default());
    }

    #[test]
    fn every_udp_argument_reaches_the_option() {
        let args = parse(&[
            "--interface",
            "eth1",
            "--heartbeat-us",
            "20000",
            "--response-timeout-ms",
            "300",
            "--enumeration-timeout-ms",
            "20000",
        ]);
        assert_eq!(
            args.option(),
            TransportOption {
                iface: Interface::Name("eth1".to_string()),
                heartbeat: Some(Duration::from_millis(20)),
                lost_timeout: Duration::from_millis(200),
                response_timeout: Duration::from_millis(300),
                enumeration_timeout: Duration::from_secs(20),
                ..TransportOption::default()
            }
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
            let message = udp_error(nothing).to_string();
            assert!(
                message.contains("firewall") && message.contains("44336"),
                "{message}"
            );
        }

        let message = udp_error(UdpError::DeviceCountMismatch {
            expected: 2,
            found: 3,
        })
        .to_string();
        assert!(!message.contains("firewall"), "{message}");
    }

    #[test]
    fn the_simulator_flag_reaches_the_option_and_excludes_an_interface() {
        let args = parse(&["--simulator"]);
        assert_eq!(args.option().iface, Interface::Simulator);
        assert!(Cli::try_parse_from(["ota", "--simulator", "--interface", "eth0"]).is_err());
    }
}
