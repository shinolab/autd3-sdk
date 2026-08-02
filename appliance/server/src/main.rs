mod api;
mod config;
mod health;
mod mdns;
#[cfg(target_os = "linux")]
mod notify;
mod process;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use autd3_rs_core::Link;
use autd3_rs_core::rt::{TracingOption, init_tracing};
use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption};
use autd3_rs_link_remote::{BusServer, Desired, RemoteLinkError, SharedBus};
use clap::Parser;

use crate::config::Config;

const DEFAULT_CONFIG_PATH: &str = "/etc/autd3/remote-server.toml";
pub const BINARY_NAME: &str = "autd3-remote-server";

#[derive(Parser)]
#[command(
    name = BINARY_NAME,
    version,
    about = "Drive the AUTD3 EtherCAT bus locally and relay frames to a remote client over TCP"
)]
struct Cli {
    /// TOML configuration file; the built-in defaults are used when it is absent
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
    #[arg(long, help = "EtherCAT interface to drive (overrides `bus.interface`)")]
    interface: Option<String>,
    /// TCP address to listen on (overrides `server.bind`)
    #[arg(long)]
    bind: Option<SocketAddr>,
    /// Open the bus once, report what is on it, and exit
    #[arg(long)]
    probe: bool,
}

fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());
    let cli = Cli::parse();

    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        tracing::info!(path = %cli.config.display(), "no config file; using the defaults");
        Config::default()
    };
    if let Some(interface) = cli.interface {
        config.bus.interface = Some(interface);
    }
    if let Some(bind) = cli.bind {
        config.server.bind = bind;
    }
    config.validate()?;

    if config.rt.lock_memory {
        process::lock_memory();
    }

    let link_option = config.link_option();
    if cli.probe {
        return probe(&link_option);
    }

    let server_option = config.server_option();
    let interface = link_option.iface.name().unwrap_or("?").to_owned();
    let sync0_period = link_option.sync0_period;
    let bus_option = config.bus_option();
    let bus = SharedBus::new(bus_option, move || {
        EchocatLink::open(&link_option).map_err(|e| RemoteLinkError::Link(e.to_string()))
    })
    .context("failed to start the bus")?;
    if config.bus.open_on_start {
        bus.set_desired(Desired::Open);
    }

    if let Some(interval) = config.health.report_interval {
        health::spawn(Arc::clone(&bus), interval)
            .context("failed to spawn the health reporting thread")?;
    }

    let rt_affinity = bus
        .applied_tuning()
        .map_or(bus_option.rt_affinity, |tuning| tuning.affinity);
    if rt_affinity.is_none() {
        tracing::warn!(
            "the bus thread is not pinned to a CPU; it will be migrated between cores under \
             load and miss SYNC0 deadlines. Set `rt.affinity` in the config file",
        );
    }
    let mut server = BusServer::new(server_option, Arc::clone(&bus))?;

    if config.control.enabled {
        let state = Arc::new(api::AppState::new(
            &config,
            cli.config.clone(),
            mdns::instance(&config),
            Arc::clone(&bus),
            server.sessions(),
        ));
        api::spawn(&config, state).context("failed to start the control API")?;
    }

    let _advertisement = mdns::advertise(&config);

    tracing::info!(
        bind = %server_option.bind,
        interface,
        ?sync0_period,
        auto_open = server_option.auto_open,
        rt_affinity = ?rt_affinity.map(|c| c.id),
        "remote master appliance ready; waiting for a client",
    );

    #[cfg(target_os = "linux")]
    {
        notify::ready();
        notify::spawn_watchdog(Arc::clone(&bus));
    }

    server.serve()?;
    Ok(())
}

fn probe(option: &EchocatLinkOption) -> Result<()> {
    let link = EchocatLink::open(option).context("failed to open the EtherCAT bus")?;
    println!(
        "{} device(s) on {}",
        link.num_devices(),
        option.iface.name().unwrap_or("?"),
    );
    Ok(())
}
