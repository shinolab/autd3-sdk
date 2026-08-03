use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs_appliance::{
    Appliance, ApplianceClient, ApplianceStatus, ConfigDocument, DEFAULT_CONTROL_PORT,
    DiscoveryOption, LogLines, UplinkStatus, WifiCredentials, WifiForget, discover_all,
};
use clap::{Parser, Subcommand};

const DEFAULT_LOG_LINES: usize = 200;

#[derive(Parser)]
#[command(
    name = "autd3-appliance",
    version,
    about = "Find the AUTD3 EtherCAT master appliance and drive its control API"
)]
struct Cli {
    /// Control API address; the appliance is looked up over mDNS when this is absent
    #[arg(long, global = true)]
    addr: Option<SocketAddr>,
    /// Instance name to pick when several appliances answer
    #[arg(long, global = true)]
    instance: Option<String>,
    /// How long to wait for mDNS answers
    #[arg(long, global = true, default_value = "2", value_name = "SECONDS")]
    discovery_timeout: u64,
    /// Print machine-readable JSON instead of a table
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan the network for appliances
    Scan,
    /// Show the bus and appliance status
    Status,
    /// Ask the bus to open
    Open,
    /// Ask the bus to close
    Close,
    /// Count the devices on the bus
    Probe,
    /// Read or replace the configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print the tail of the appliance journal
    Logs {
        #[arg(short = 'n', long, default_value_t = DEFAULT_LOG_LINES)]
        lines: usize,
        /// Unit to read; the server's own unit by default
        #[arg(long)]
        unit: Option<String>,
    },
    /// Restart the server process
    Restart,
    /// Reboot the appliance
    Reboot,
    /// Power the appliance off
    Shutdown,
    /// Replace the server binary and restart it
    Update {
        /// Server binary built for the appliance
        binary: PathBuf,
    },
    /// Set up or tear down Wi-Fi
    Wifi {
        #[command(subcommand)]
        action: WifiAction,
    },
}

#[derive(Subcommand)]
enum WifiAction {
    /// Store Wi-Fi credentials
    Set {
        #[arg(long)]
        ssid: String,
        /// Passphrase
        #[arg(long)]
        psk: Option<String>,
        /// The network has no passphrase
        #[arg(long, conflicts_with = "psk")]
        open: bool,
        /// Regulatory domain, e.g. JP
        #[arg(long)]
        country: Option<String>,
    },
    /// Remove the stored credentials
    Forget {
        /// Also keep the radio off across reboots
        #[arg(long)]
        radio_off: bool,
        /// Go ahead even without a wired uplink, losing contact with the appliance
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the current configuration
    Get,
    /// Replace the configuration; it takes effect on the next restart
    Set {
        /// TOML file to upload, or `-` to read standard input
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Command::Scan) {
        return scan(&cli);
    }

    let client = connect(&cli)?;
    match &cli.command {
        Command::Scan => unreachable!("handled above"),
        Command::Status => status(&cli, &client)?,
        Command::Open => emit(&cli, &client.bus_open()?, |a| println!("{}", a.message))?,
        Command::Close => emit(&cli, &client.bus_close()?, |a| println!("{}", a.message))?,
        Command::Probe => emit(&cli, &client.bus_probe()?, |result| {
            println!("{} device(s) on the bus", result.num_devices);
        })?,
        Command::Config { action } => match action {
            ConfigAction::Get => {
                let document = ConfigDocument {
                    toml: client.config()?,
                };
                emit(&cli, &document, |document| print!("{}", document.toml))?;
            }
            ConfigAction::Set { file } => {
                let toml = read_input(file)?;
                emit(&cli, &client.set_config(&toml)?, |a| {
                    println!("{}", a.message);
                })?;
            }
        },
        Command::Logs { lines, unit } => {
            let logs = LogLines {
                lines: client.logs(unit.as_deref(), *lines)?,
            };
            emit(&cli, &logs, |logs| {
                for line in &logs.lines {
                    println!("{line}");
                }
            })?;
        }
        Command::Restart => emit(&cli, &client.restart()?, |a| println!("{}", a.message))?,
        Command::Reboot => emit(&cli, &client.reboot()?, |a| println!("{}", a.message))?,
        Command::Shutdown => emit(&cli, &client.shutdown()?, |a| println!("{}", a.message))?,
        Command::Update { binary } => {
            let bytes =
                std::fs::read(binary).with_context(|| format!("reading {}", binary.display()))?;
            if !cli.json {
                println!("uploading {} bytes", bytes.len());
            }
            emit(&cli, &client.update(&bytes)?, |a| println!("{}", a.message))?;
        }
        Command::Wifi { action } => match action {
            WifiAction::Set {
                ssid,
                psk,
                open,
                country,
            } => {
                if psk.is_none() && !open {
                    bail!(
                        "pass --psk <PASSPHRASE>, or --open if `{ssid}` really has no passphrase. \
                         An open profile cannot associate with a protected network, and the board \
                         only says `ssid-not-found` when it fails",
                    );
                }
                let credentials = WifiCredentials {
                    ssid: ssid.clone(),
                    psk: psk.clone(),
                    country: country.clone(),
                };
                emit(&cli, &client.set_wifi(&credentials)?, |a| {
                    println!("{}", a.message);
                })?;
            }
            WifiAction::Forget { radio_off, force } => {
                let request = WifiForget {
                    radio_off: *radio_off,
                    force: *force,
                };
                emit(&cli, &client.forget_wifi(&request)?, |a| {
                    println!("{}", a.message);
                })?;
            }
        },
    }
    Ok(())
}

fn emit<T: serde::Serialize>(cli: &Cli, value: &T, plain: impl FnOnce(&T)) -> Result<()> {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        plain(value);
    }
    Ok(())
}

fn discovery_option(cli: &Cli) -> DiscoveryOption {
    DiscoveryOption {
        timeout: Duration::from_secs(cli.discovery_timeout),
        instance: cli.instance.clone(),
    }
}

fn scan(cli: &Cli) -> Result<()> {
    let found = discover_all(&discovery_option(cli))?;
    if cli.json {
        let rows: Vec<_> = found.iter().map(json_of).collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if found.is_empty() {
        match &cli.instance {
            Some(instance) => println!("no appliance called `{instance}` answered"),
            None => println!("no appliance answered"),
        }
        return Ok(());
    }
    for appliance in &found {
        println!(
            "{}\t{}\tcontrol {}\twire {}\tsdk {}",
            appliance.instance,
            appliance.addr,
            appliance
                .control_port
                .map_or_else(|| "-".to_owned(), |port| port.to_string()),
            appliance
                .wire
                .map_or_else(|| "-".to_owned(), |wire| wire.to_string()),
            appliance.sdk.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}

fn json_of(appliance: &Appliance) -> serde_json::Value {
    serde_json::json!({
        "instance": appliance.instance,
        "host": appliance.host,
        "addr": appliance.addr.to_string(),
        "addresses": appliance.addresses.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "control_port": appliance.control_port,
        "wire": appliance.wire,
        "sdk": appliance.sdk,
    })
}

fn control_addr(appliance: &Appliance) -> SocketAddr {
    let mut addr = appliance.addr;
    addr.set_port(appliance.control_port.unwrap_or(DEFAULT_CONTROL_PORT));
    addr
}

fn connect(cli: &Cli) -> Result<ApplianceClient> {
    if let Some(addr) = cli.addr {
        return Ok(ApplianceClient::new(addr));
    }
    let found = discover_all(&discovery_option(cli))?;
    match found.len() {
        0 => match &cli.instance {
            Some(instance) => bail!(
                "no appliance called `{instance}` answered within {}s. \
                 Run `autd3-appliance scan` without --instance to see what is on this link",
                cli.discovery_timeout,
            ),
            None => bail!(
                "no appliance answered within {}s. \
                 Pass --addr <host:port> if it is not on this link",
                cli.discovery_timeout,
            ),
        },
        1 => {
            let appliance = &found[0];
            eprintln!("using {} at {}", appliance.instance, appliance.addr);
            Ok(ApplianceClient::new(control_addr(appliance)))
        }
        _ => {
            let names: Vec<_> = found.iter().map(|a| a.instance.clone()).collect();
            bail!(
                "{} appliances answered ({}); pick one with --instance",
                found.len(),
                names.join(", "),
            )
        }
    }
}

fn read_input(file: &PathBuf) -> Result<String> {
    if file.as_os_str() == "-" {
        return std::io::read_to_string(std::io::stdin())
            .context("reading the configuration from standard input");
    }
    std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))
}

fn status(cli: &Cli, client: &ApplianceClient) -> Result<()> {
    let status = client.status()?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }
    print_status(&status);
    Ok(())
}

fn print_status(status: &ApplianceStatus) {
    let bus = &status.bus;
    println!("instance   {}", status.instance);
    println!(
        "versions   autd3-sdk {} / wire {}",
        status.sdk_version, status.wire_version,
    );
    if let Some(image) = &status.image {
        println!(
            "image      {} (built {}, {}, autd3-sdk {})",
            image.version, image.built, image.board, image.sdk_version,
        );
    }
    if let Some(binary) = &status.binary {
        println!("binary     {binary}");
    }
    println!("uptime     {}", human_duration(status.uptime_secs));
    println!(
        "bus        {:?} (requested {:?}){}",
        bus.actual,
        bus.desired,
        bus.failure
            .as_ref()
            .map_or_else(String::new, |reason| format!(": {reason}")),
    );
    println!(
        "devices    {} [{}]",
        bus.num_devices,
        bus.devices.join(", "),
    );
    println!(
        "counters   recoveries {} / stale {} / lost {} / phase excursions {} (worst {})",
        bus.recoveries,
        bus.stale_cycles,
        bus.lost_cycles,
        bus.phase_excursions,
        micros(bus.worst_phase_deviation_ns),
    );
    if bus.exchanges > 0 {
        println!(
            "exchange   mean {} / worst {} over {} cycles",
            micros(bus.exchange_mean_ns),
            micros(bus.exchange_worst_ns),
            bus.exchanges,
        );
    }
    println!(
        "ethercat   {} {}{}",
        status.interface.name,
        if status.interface.carrier {
            "up"
        } else {
            status.interface.operstate.as_str()
        },
        status
            .interface
            .speed_mbps
            .map_or_else(String::new, |speed| format!(" ({speed} Mbit/s)")),
    );
    if status.uplinks.is_empty() {
        println!("uplink     none");
    }
    for uplink in &status.uplinks {
        println!("uplink     {}", uplink_line(uplink));
    }
    if let Some(storage) = &status.storage {
        println!(
            "storage    {} {} MB free of {} MB",
            storage.path, storage.free_mb, storage.total_mb,
        );
    }
    match &status.client {
        Some(client) => println!(
            "client     {} ({} devices, {})",
            client.peer,
            client.devices,
            human_duration(client.connected_secs),
        ),
        None => println!("client     none"),
    }
    if !status.allow_admin {
        println!("note       administrative endpoints are disabled");
    }
}

fn uplink_line(uplink: &UplinkStatus) -> String {
    let mut parts = vec![
        uplink.name.clone(),
        if uplink.carrier {
            "up".to_owned()
        } else {
            uplink.operstate.clone()
        },
    ];
    if let Some(wifi) = &uplink.wifi {
        parts.push(match (&wifi.ssid, wifi.signal_dbm) {
            (Some(ssid), Some(dbm)) => format!("{ssid} ({dbm} dBm)"),
            (Some(ssid), None) => ssid.clone(),
            (None, _) if wifi.blocked => "radio blocked".to_owned(),
            (None, _) => "not associated".to_owned(),
        });
        parts.push(format!(
            "domain {}",
            wifi.regdomain.as_deref().unwrap_or("unset"),
        ));
    }
    parts.push(if uplink.addresses.is_empty() {
        "no address".to_owned()
    } else {
        uplink.addresses.join(", ")
    });
    parts.join(" / ")
}

fn micros(ns: u64) -> String {
    format!("{} us", ns / 1_000)
}

fn human_duration(secs: u64) -> String {
    let (days, hours, minutes) = (secs / 86_400, (secs % 86_400) / 3_600, (secs % 3_600) / 60);
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m {}s", secs % 60)
    }
}
