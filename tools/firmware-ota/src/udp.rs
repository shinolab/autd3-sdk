use std::net::SocketAddrV6;
use std::time::Duration;

use clap::Args;

use autd3_rs::{TransportOption, UdpBus, UdpError};

const MAX_DEVICES: usize = 255;
const LOST_AFTER_HEARTBEATS: u32 = 10;

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "UDP (unset options keep the library defaults)")]
pub struct UdpArgs {
    #[arg(long, help = "Network interface name (default: auto-detect)")]
    pub interface: Option<String>,
    #[arg(
        long,
        conflicts_with = "interface",
        help = "Send the multicast management messages here instead of ff02::1 (e.g. the simulator's [::1]:44336)"
    )]
    pub group: Option<SocketAddrV6>,
    #[arg(long, help = "Heartbeat interval while waiting, microseconds")]
    pub heartbeat_us: Option<u64>,
    #[arg(
        long,
        help = "How long a heartbeat waits for every reply, microseconds"
    )]
    pub reply_timeout_us: Option<u64>,
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
    #[arg(
        long,
        help = "How long to wait for every device to lock to the grandmaster and start its sync pulse, milliseconds"
    )]
    pub sync_timeout_ms: Option<u64>,
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
        let heartbeat = us(self.heartbeat_us).unwrap_or(default.heartbeat);
        TransportOption {
            iface: self.interface.clone().into(),
            group: self.group,
            heartbeat,
            reply_timeout: us(self.reply_timeout_us).unwrap_or(default.reply_timeout),
            lost_timeout: default
                .lost_timeout
                .max(heartbeat.saturating_mul(LOST_AFTER_HEARTBEATS)),
            response_timeout: ms(self.response_timeout_ms).unwrap_or(default.response_timeout),
            enumeration_timeout: ms(self.enumeration_timeout_ms)
                .unwrap_or(default.enumeration_timeout),
            sync_timeout: ms(self.sync_timeout_ms).unwrap_or(default.sync_timeout),
        }
    }

    pub fn open(&self, expected: Option<usize>) -> Result<UdpBus, UdpError> {
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use autd3_rs::Interface;

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
            "--reply-timeout-us",
            "1500",
            "--response-timeout-ms",
            "300",
            "--enumeration-timeout-ms",
            "20000",
            "--sync-timeout-ms",
            "8000",
        ]);
        assert_eq!(
            args.option(),
            TransportOption {
                iface: Interface::Name("eth1".to_string()),
                heartbeat: Duration::from_millis(20),
                reply_timeout: Duration::from_micros(1500),
                lost_timeout: Duration::from_millis(200),
                response_timeout: Duration::from_millis(300),
                enumeration_timeout: Duration::from_secs(20),
                sync_timeout: Duration::from_secs(8),
                ..TransportOption::default()
            }
        );
    }

    #[test]
    fn a_group_address_reaches_the_option_and_excludes_an_interface() {
        let args = parse(&["--group", "[::1]:44336"]);
        assert_eq!(args.option().group, Some("[::1]:44336".parse().unwrap()));
        assert!(
            Cli::try_parse_from(["ota", "--group", "[::1]:44336", "--interface", "eth0"]).is_err()
        );
    }
}
