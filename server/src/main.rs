use std::net::{Ipv6Addr, SocketAddrV6};
use std::num::NonZeroUsize;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs::SERVER_PORT;
use autd3_rs::rt::{LogWriter, TracingOption, init_tracing};
use autd3_rs::udp::UdpOption;
use autd3_rs_server::{Server, ServerOption};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "autd3-server",
    about,
    long_about = "Keep the AUTD3 devices enumerated and synchronized, and relay a client on this host to them.\n\n\
        A client connects with `Link::Server(ServerLink { port, .. })` over TCP on the loopback address. The \
        server owns the UDP options (heartbeat, lost timeout, send rate limit), so reopening a \
        client skips the enumeration and the PTP lock wait of the devices.\n\n\
        The server sends heartbeats only while a client is attached and never sends a frame of its own: \
        while no client is attached the devices stop their output through their failsafe.\n\n\
        Logs go to stderr. One JSON line describing the devices goes to stdout every monitor interval."
)]
struct Cli {
    #[arg(
        long,
        help = "Number of devices on the chain (default: whatever the enumeration finds)"
    )]
    devices: Option<NonZeroUsize>,
    #[arg(
        long,
        help = "Network interface the devices are attached to (default: the one that answers)"
    )]
    iface: Option<String>,
    #[arg(long, default_value_t = SERVER_PORT, value_parser = clap::value_parser!(u16).range(1..), help = "Loopback port the client connects to")]
    port: u16,
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration, help = "Interval of the unit info reads")]
    monitor_interval: Duration,
    #[arg(long, value_parser = humantime::parse_duration, conflicts_with = "no_heartbeat", help = "Heartbeat interval while a client is attached (default: 10ms)")]
    heartbeat: Option<Duration>,
    #[arg(
        long,
        help = "Send no heartbeat; only the frames of the client keep the devices alive"
    )]
    no_heartbeat: bool,
    #[arg(long, value_parser = humantime::parse_duration, help = "Wait for the heartbeat replies (default: 1ms)")]
    reply_timeout: Option<Duration>,
    #[arg(long, value_parser = humantime::parse_duration, help = "Silence after which a device is lost (default: 100ms)")]
    lost_timeout: Option<Duration>,
    #[arg(long, value_parser = humantime::parse_duration, help = "Wait for the replies of the management messages (default: 200ms)")]
    response_timeout: Option<Duration>,
    #[arg(long, value_parser = humantime::parse_duration, help = "Give up the enumeration when the device count is not reached within this time (default: 10s)")]
    enumeration_timeout: Option<Duration>,
    #[arg(long, value_parser = humantime::parse_duration, help = "Give up when the devices do not lock to PTP within this time (default: 30s)")]
    sync_timeout: Option<Duration>,
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration, help = "Pause before enumerating again after a failure")]
    retry_interval: Duration,
    #[arg(
        long,
        help = "Cap of the send rate towards the devices in percent of the link (default: no cap)"
    )]
    send_rate_limit: Option<f32>,
    #[arg(
        long,
        help = "Socket send buffer in bytes; 0 keeps the OS default (default: 8192)"
    )]
    send_buffer: Option<usize>,
    #[arg(long, value_parser = humantime::parse_duration, conflicts_with = "no_timer_resolution", help = "Windows timer resolution held while the devices are open (default: 1ms)")]
    timer_resolution: Option<Duration>,
    #[arg(long, help = "Leave the Windows timer resolution at the OS default")]
    no_timer_resolution: bool,
}

impl Cli {
    fn validate(&self) -> Result<()> {
        if self.monitor_interval.is_zero() {
            bail!("--monitor-interval must not be zero");
        }
        if self.retry_interval.is_zero() {
            bail!("--retry-interval must not be zero");
        }
        Ok(())
    }

    fn option(&self) -> ServerOption {
        let defaults = UdpOption::default();
        ServerOption {
            transport: UdpOption {
                iface: self.iface.clone().into(),
                heartbeat: if self.no_heartbeat {
                    None
                } else {
                    self.heartbeat.or(defaults.heartbeat)
                },
                reply_timeout: self.reply_timeout.unwrap_or(defaults.reply_timeout),
                lost_timeout: self.lost_timeout.unwrap_or(defaults.lost_timeout),
                response_timeout: self.response_timeout.unwrap_or(defaults.response_timeout),
                enumeration_timeout: self
                    .enumeration_timeout
                    .unwrap_or(defaults.enumeration_timeout),
                sync_timeout: self.sync_timeout.unwrap_or(defaults.sync_timeout),
                send_rate_limit: self.send_rate_limit,
                send_buffer: self
                    .send_buffer
                    .map_or(defaults.send_buffer, NonZeroUsize::new),
                timer_resolution: if self.no_timer_resolution {
                    None
                } else {
                    self.timer_resolution.or(defaults.timer_resolution)
                },
            },
            devices: self.devices,
            listen: SocketAddrV6::new(Ipv6Addr::LOCALHOST, self.port, 0, 0),
            monitor_interval: self.monitor_interval,
            retry_interval: self.retry_interval,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate()?;
    let _log_guard = init_tracing(TracingOption {
        writer: LogWriter::Stderr,
        ..TracingOption::default()
    });
    let server = Server::bind(cli.option())
        .with_context(|| format!("binding the loopback port {}", cli.port))?;
    tracing::info!(addr = %server.addr(), "waiting for a client");
    let stop = AtomicBool::new(false);
    server.run(&stop, |status| match status.to_json() {
        Ok(json) => println!("{json}"),
        Err(e) => tracing::error!("serializing the status failed: {e}"),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs::Interface;

    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("autd3-server").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn no_arguments_mean_the_library_defaults() {
        assert_eq!(parse(&[]).option(), ServerOption::default());
    }

    #[test]
    fn every_argument_reaches_the_option() {
        let option = parse(&[
            "--devices",
            "4",
            "--iface",
            "eth0",
            "--port",
            "5000",
            "--monitor-interval",
            "250ms",
            "--retry-interval",
            "3s",
            "--heartbeat",
            "20ms",
            "--reply-timeout",
            "2ms",
            "--lost-timeout",
            "150ms",
            "--send-rate-limit",
            "95",
            "--response-timeout",
            "300ms",
            "--enumeration-timeout",
            "20s",
            "--sync-timeout",
            "1m",
            "--send-buffer",
            "4096",
            "--timer-resolution",
            "2ms",
        ])
        .option();
        assert_eq!(
            option,
            ServerOption {
                transport: UdpOption {
                    iface: Interface::Name("eth0".into()),
                    heartbeat: Some(Duration::from_millis(20)),
                    reply_timeout: Duration::from_millis(2),
                    lost_timeout: Duration::from_millis(150),
                    response_timeout: Duration::from_millis(300),
                    enumeration_timeout: Duration::from_secs(20),
                    sync_timeout: Duration::from_secs(60),
                    send_rate_limit: Some(95.0),
                    send_buffer: NonZeroUsize::new(4096),
                    timer_resolution: Some(Duration::from_millis(2)),
                },
                devices: NonZeroUsize::new(4),
                listen: "[::1]:5000".parse().unwrap(),
                monitor_interval: Duration::from_millis(250),
                retry_interval: Duration::from_secs(3),
            }
        );
    }

    #[test]
    fn the_heartbeat_can_be_turned_off_but_not_both_ways_at_once() {
        assert_eq!(
            parse(&["--no-heartbeat"]).option().transport.heartbeat,
            None
        );
        assert!(
            Cli::try_parse_from(["autd3-server", "--no-heartbeat", "--heartbeat", "10ms"]).is_err()
        );
    }

    #[test]
    fn the_send_buffer_and_the_timer_resolution_can_be_left_to_the_os() {
        let option = parse(&["--send-buffer", "0", "--no-timer-resolution"]).option();
        assert_eq!(option.transport.send_buffer, None);
        assert_eq!(option.transport.timer_resolution, None);
        assert!(
            Cli::try_parse_from([
                "autd3-server",
                "--no-timer-resolution",
                "--timer-resolution",
                "1ms"
            ])
            .is_err()
        );
    }

    #[test]
    fn zero_intervals_are_rejected() {
        assert!(parse(&[]).validate().is_ok());
        assert_eq!(
            parse(&["--monitor-interval", "0s"])
                .validate()
                .unwrap_err()
                .to_string(),
            "--monitor-interval must not be zero"
        );
        assert_eq!(
            parse(&["--retry-interval", "0s"])
                .validate()
                .unwrap_err()
                .to_string(),
            "--retry-interval must not be zero"
        );
        assert!(parse(&["--retry-interval", "1ms"]).validate().is_ok());
    }

    #[test]
    fn the_port_must_not_be_left_to_the_os() {
        assert!(Cli::try_parse_from(["autd3-server", "--port", "0"]).is_err());
        assert_eq!(parse(&["--port", "1"]).port, 1);
    }

    #[test]
    fn the_simulator_is_not_an_option() {
        assert!(Cli::try_parse_from(["autd3-server", "--simulator"]).is_err());
    }
}
