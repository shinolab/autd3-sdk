use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use autd3_rs_core::geometry::{Autd3, Geometry};
use autd3_rs_core::link::Link;
use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption, FramePhase, SleepStrategy};
use autd3_rs_link_remote::{DiscoveryOption, RemoteLink, RemoteLinkOption, ServerKind, discover};
use autd3_rs_link_twincat::{AmsNetId, Timeouts, TwinCATLink, TwinCATLinkOption};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Echocat,
    Twincat,
    Remote,
}

#[derive(Args, Debug, Clone)]
#[command(next_help_heading = "Link")]
pub struct LinkArgs {
    #[arg(
        long,
        value_enum,
        default_value_t = LinkKind::Echocat,
        help = "EtherCAT master to reach the devices with"
    )]
    pub link: LinkKind,
    #[arg(
        long,
        help = "echocat SYNC0 period in microseconds (default: the link default, 2000 on Windows and 1000 elsewhere)"
    )]
    pub cycle_us: Option<u64>,
    #[command(flatten)]
    pub echocat: EchocatArgs,
    #[command(flatten)]
    pub twincat: TwinCatArgs,
    #[command(flatten)]
    pub remote: RemoteArgs,
}

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "echocat (unset options keep the link defaults)")]
pub struct EchocatArgs {
    #[arg(long, help = "Network interface name (default: auto-detect)")]
    pub interface: Option<String>,
    #[arg(
        long,
        help = "Frame exchange phase within the SYNC0 period, microseconds (default: automatic)"
    )]
    pub frame_phase_us: Option<u64>,
    #[arg(long, help = "PDU response timeout, milliseconds")]
    pub pdu_timeout_ms: Option<u64>,
    #[arg(long, help = "AL state transition timeout, milliseconds")]
    pub state_transition_timeout_ms: Option<u64>,
    #[arg(long, help = "Number of DC static drift compensation iterations")]
    pub dc_static_sync_iterations: Option<u32>,
    #[arg(long, help = "Delay before SYNC0 starts, milliseconds")]
    pub dc_start_delay_ms: Option<u64>,
    #[arg(long, help = "DC sync tolerance, microseconds")]
    pub sync_tolerance_us: Option<u64>,
    #[arg(long, help = "DC sync timeout, milliseconds")]
    pub sync_timeout_ms: Option<u64>,
    #[arg(long, help = "Process data watchdog, milliseconds")]
    pub process_data_watchdog_ms: Option<u64>,
    #[arg(
        long,
        help = "Spin instead of sleeping for the last N microseconds before each frame"
    )]
    pub spin_margin_us: Option<u64>,
}

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "TwinCAT")]
pub struct TwinCatArgs {
    #[arg(
        long,
        requires = "ams_net_id",
        help = "IP address of a remote TwinCAT runtime (default: the local runtime)"
    )]
    pub twincat_remote: Option<IpAddr>,
    #[arg(
        long,
        requires = "twincat_remote",
        help = "AMS Net ID of the remote TwinCAT runtime"
    )]
    pub ams_net_id: Option<AmsNetId>,
    #[arg(long, help = "ADS connect timeout, milliseconds")]
    pub twincat_connect_timeout_ms: Option<u64>,
    #[arg(long, help = "ADS read timeout, milliseconds")]
    pub twincat_read_timeout_ms: Option<u64>,
    #[arg(long, help = "ADS write timeout, milliseconds")]
    pub twincat_write_timeout_ms: Option<u64>,
}

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "Remote")]
pub struct RemoteArgs {
    #[arg(
        long,
        help = "Address of the remote link server (appliance / simulator) (default: find the appliance over mDNS)"
    )]
    pub remote_addr: Option<SocketAddr>,
    #[arg(
        long,
        conflicts_with = "remote_addr",
        help = "Instance name of the appliance to pick when several answer over mDNS"
    )]
    pub remote_instance: Option<String>,
    #[arg(
        long,
        conflicts_with = "remote_addr",
        help = "How long to wait for the appliance to answer over mDNS, milliseconds (default: the discovery default)"
    )]
    pub discovery_timeout_ms: Option<u64>,
    #[arg(long, help = "Remote request timeout, milliseconds (default: none)")]
    pub remote_timeout_ms: Option<u64>,
}

#[must_use]
pub fn geometry(devices: usize) -> Geometry {
    Geometry::new((0..devices).map(|_| Autd3::default()).collect())
}

fn check_devices(found: usize, expected: Option<usize>) -> Result<()> {
    match expected {
        Some(expected) if expected != found => {
            anyhow::bail!("--devices is {expected}, but the link reports {found} device(s)")
        }
        _ => Ok(()),
    }
}

fn ms(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_millis)
}

fn us(value: Option<u64>) -> Option<Duration> {
    value.map(Duration::from_micros)
}

impl LinkArgs {
    #[must_use]
    pub fn echocat(&self) -> EchocatLinkOption {
        let e = &self.echocat;
        let default = EchocatLinkOption::default();
        EchocatLinkOption {
            iface: e.interface.clone().into(),
            sync0_period: us(self.cycle_us).unwrap_or(default.sync0_period),
            frame_phase: us(e.frame_phase_us).map_or(default.frame_phase, FramePhase::At),
            pdu_timeout: ms(e.pdu_timeout_ms).unwrap_or(default.pdu_timeout),
            state_transition_timeout: ms(e.state_transition_timeout_ms)
                .unwrap_or(default.state_transition_timeout),
            dc_static_sync_iterations: e
                .dc_static_sync_iterations
                .unwrap_or(default.dc_static_sync_iterations),
            dc_start_delay: ms(e.dc_start_delay_ms).unwrap_or(default.dc_start_delay),
            sync_tolerance: us(e.sync_tolerance_us).unwrap_or(default.sync_tolerance),
            sync_timeout: ms(e.sync_timeout_ms).unwrap_or(default.sync_timeout),
            process_data_watchdog: ms(e.process_data_watchdog_ms)
                .unwrap_or(default.process_data_watchdog),
            sleep_strategy: us(e.spin_margin_us).map_or(default.sleep_strategy, |margin| {
                SleepStrategy::Spin { margin }
            }),
        }
    }

    #[must_use]
    pub fn twincat(&self) -> TwinCATLinkOption {
        let t = &self.twincat;
        let timeouts = Timeouts {
            connect: ms(t.twincat_connect_timeout_ms),
            read: ms(t.twincat_read_timeout_ms),
            write: ms(t.twincat_write_timeout_ms),
        };
        match (t.twincat_remote, t.ams_net_id) {
            (Some(addr), Some(ams_net_id)) => {
                TwinCATLinkOption::remote_with_timeouts(addr, ams_net_id, timeouts)
            }
            _ => TwinCATLinkOption::local_with_timeouts(timeouts),
        }
    }

    pub fn open_echocat(&self, expected: Option<usize>) -> Result<EchocatLink> {
        let link = EchocatLink::open(&self.echocat())?;
        check_devices(link.num_devices(), expected)?;
        Ok(link)
    }

    pub fn open_twincat(&self, expected: Option<usize>) -> Result<TwinCATLink> {
        let link = TwinCATLink::open(self.twincat())?;
        check_devices(link.num_devices(), expected)?;
        Ok(link)
    }

    pub fn resolve_remote(&mut self) -> Result<()> {
        if self.link != LinkKind::Remote || self.remote.remote_addr.is_some() {
            return Ok(());
        }
        let appliance = discover(&self.discovery())
            .context("finding the appliance over mDNS (or pass --remote-addr)")?;
        println!("appliance: {appliance}");
        self.remote.remote_addr = Some(appliance.addr);
        Ok(())
    }

    #[must_use]
    fn discovery(&self) -> DiscoveryOption {
        let default = DiscoveryOption::default();
        DiscoveryOption {
            timeout: ms(self.remote.discovery_timeout_ms).unwrap_or(default.timeout),
            instance: self.remote.remote_instance.clone(),
            kind: Some(ServerKind::Appliance),
        }
    }

    pub fn open_remote(&self, expected: Option<usize>) -> Result<RemoteLink> {
        let option = self.remote()?;
        let devices = expected.unwrap_or(1);
        match RemoteLink::open(option.addr, option.timeout, &geometry(devices)) {
            Ok(link) => Ok(link),
            Err(e) => {
                let bus = e
                    .rejected_device_count()
                    .filter(|_| expected.is_none())
                    .context(e)?;
                Ok(RemoteLink::open(
                    option.addr,
                    option.timeout,
                    &geometry(bus),
                )?)
            }
        }
    }

    fn remote(&self) -> Result<RemoteLinkOption> {
        Ok(RemoteLinkOption {
            addr: self
                .remote
                .remote_addr
                .context("the remote link server address is not resolved")?,
            timeout: ms(self.remote.remote_timeout_ms),
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use autd3_rs_core::link::Interface;
    use autd3_rs_link_twincat::TwinCATServer;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        link: LinkArgs,
    }

    fn parse(args: &[&str]) -> LinkArgs {
        Cli::try_parse_from(std::iter::once("ota").chain(args.iter().copied()))
            .unwrap()
            .link
    }

    #[test]
    fn no_echocat_arguments_mean_the_link_defaults() {
        let args = parse(&[]);
        assert_eq!(args.echocat(), EchocatLinkOption::default());
    }

    #[test]
    fn every_echocat_argument_reaches_the_option() {
        let args = parse(&[
            "--interface",
            "eth1",
            "--cycle-us",
            "2000",
            "--frame-phase-us",
            "300",
            "--pdu-timeout-ms",
            "50",
            "--state-transition-timeout-ms",
            "20000",
            "--dc-static-sync-iterations",
            "500",
            "--dc-start-delay-ms",
            "200",
            "--sync-tolerance-us",
            "5",
            "--sync-timeout-ms",
            "30000",
            "--process-data-watchdog-ms",
            "250",
            "--spin-margin-us",
            "700",
        ]);
        assert_eq!(
            args.echocat(),
            EchocatLinkOption {
                iface: Interface::Name("eth1".to_string()),
                sync0_period: Duration::from_millis(2),
                frame_phase: FramePhase::At(Duration::from_micros(300)),
                pdu_timeout: Duration::from_millis(50),
                state_transition_timeout: Duration::from_secs(20),
                dc_static_sync_iterations: 500,
                dc_start_delay: Duration::from_millis(200),
                sync_tolerance: Duration::from_micros(5),
                sync_timeout: Duration::from_secs(30),
                process_data_watchdog: Duration::from_millis(250),
                sleep_strategy: SleepStrategy::Spin {
                    margin: Duration::from_micros(700)
                },
            }
        );
    }

    #[test]
    fn a_remote_twincat_server_needs_both_its_address_and_net_id() {
        let args = parse(&[
            "--link",
            "twincat",
            "--twincat-remote",
            "192.168.1.2",
            "--ams-net-id",
            "1.2.3.4.1.1",
            "--twincat-read-timeout-ms",
            "1500",
        ]);
        let option = args.twincat();
        assert!(matches!(
            option.server,
            TwinCATServer::Remote { addr, .. } if addr == "192.168.1.2".parse::<IpAddr>().unwrap()
        ));
        assert_eq!(option.timeouts.read, Some(Duration::from_millis(1500)));
        assert_eq!(option.timeouts.connect, None);

        assert!(matches!(parse(&[]).twincat().server, TwinCATServer::Local));
        assert!(Cli::try_parse_from(["ota", "--twincat-remote", "192.168.1.2"]).is_err());
        assert!(Cli::try_parse_from(["ota", "--ams-net-id", "1.2.3.4.1.1"]).is_err());
    }

    #[test]
    fn the_remote_option_takes_its_address_and_timeout() {
        let option = parse(&[
            "--link",
            "remote",
            "--remote-addr",
            "10.0.0.5:9000",
            "--remote-timeout-ms",
            "800",
        ])
        .remote()
        .unwrap();
        assert_eq!(option.addr, "10.0.0.5:9000".parse().unwrap());
        assert_eq!(option.timeout, Some(Duration::from_millis(800)));
        assert!(parse(&[]).remote().is_err());
    }

    #[test]
    fn an_explicit_remote_address_skips_the_discovery() {
        let mut args = parse(&["--link", "remote", "--remote-addr", "10.0.0.5:9000"]);
        args.resolve_remote().unwrap();
        assert_eq!(
            args.remote.remote_addr,
            Some("10.0.0.5:9000".parse().unwrap())
        );
    }

    #[test]
    fn other_links_never_discover_an_appliance() {
        let mut args = parse(&[]);
        args.resolve_remote().unwrap();
        assert_eq!(args.remote.remote_addr, None);
    }

    #[test]
    fn the_discovery_looks_for_the_named_appliance_only() {
        let option = parse(&[
            "--link",
            "remote",
            "--remote-instance",
            "autd3-lab",
            "--discovery-timeout-ms",
            "5000",
        ])
        .discovery();
        assert_eq!(
            option,
            DiscoveryOption {
                timeout: Duration::from_secs(5),
                instance: Some("autd3-lab".to_string()),
                kind: Some(ServerKind::Appliance),
            }
        );
        assert_eq!(
            parse(&[]).discovery().timeout,
            DiscoveryOption::default().timeout
        );
        assert!(
            Cli::try_parse_from([
                "ota",
                "--remote-addr",
                "10.0.0.5:9000",
                "--remote-instance",
                "autd3-lab"
            ])
            .is_err()
        );
    }
}
