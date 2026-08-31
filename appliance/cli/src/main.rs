use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs_appliance::{
    Appliance, ApplianceClient, ApplianceStatus, ConfigDocument, DEFAULT_CONTROL_PORT,
    DiscoveryOption, FRAME_PHASE_AUTO, LogLines, TuneCandidate, TuneReport, TuneRequest,
    TuneStatus, UNKNOWN_STATE_HINT, UplinkStatus, WifiCredentials, WifiForget, discover_all,
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
    /// Sweep SYNC0 periods and frame phases, and report which held OP
    Tune {
        #[command(subcommand)]
        action: TuneAction,
    },
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
enum TuneAction {
    /// Run a sweep and report the result
    Run(TuneRunArgs),
    /// Show the running or last sweep
    Status,
    /// Stop the running sweep
    Cancel,
    /// Write a measured candidate into the appliance configuration
    Apply {
        /// Candidate index from `tune status`; the recommended one by default
        #[arg(long)]
        candidate: Option<usize>,
    },
}

#[derive(clap::Args)]
struct TuneRunArgs {
    /// SYNC0 period to try; repeat the flag for a sweep
    #[arg(long = "period", value_parser = humantime::parse_duration, default_values = tune_default_periods())]
    periods: Vec<Duration>,
    /// Frame landing phase as a percent of the period, or `auto`; repeat the flag for a sweep
    #[arg(long = "frame-phase", value_parser = parse_frame_phase, default_values = tune_default_frame_phases())]
    frame_phases: Vec<u8>,
    /// Time to let each candidate settle before it is measured
    #[arg(long, value_parser = humantime::parse_duration, default_value = tune_default_ns(|r| r.warmup_ns))]
    warmup: Duration,
    /// Time each candidate is measured for
    #[arg(long, value_parser = humantime::parse_duration, default_value = tune_default_ns(|r| r.dwell_ns))]
    dwell: Duration,
    /// How long the bus is given to open or close between candidates
    #[arg(long, value_parser = humantime::parse_duration, default_value = tune_default_ns(|r| r.settle_ns))]
    settle: Duration,
    /// How often the bus state is sampled
    #[arg(long, value_parser = humantime::parse_duration, default_value = tune_default_ns(|r| r.poll_ns))]
    poll: Duration,
    /// Start the sweep and return instead of waiting for it
    #[arg(long)]
    detach: bool,
}

fn tune_default_ns(pick: fn(&TuneRequest) -> u64) -> String {
    fmt_ns(pick(&TuneRequest::default()))
}

fn tune_default_periods() -> Vec<String> {
    TuneRequest::default()
        .periods_ns
        .iter()
        .map(|&ns| fmt_ns(ns))
        .collect()
}

fn tune_default_frame_phases() -> Vec<String> {
    TuneRequest::default()
        .frame_phase_percents
        .iter()
        .map(|&percent| {
            if percent == FRAME_PHASE_AUTO {
                "auto".to_owned()
            } else {
                format!("{percent}%")
            }
        })
        .collect()
}

fn parse_frame_phase(text: &str) -> Result<u8, String> {
    if text.eq_ignore_ascii_case("auto") {
        return Ok(FRAME_PHASE_AUTO);
    }
    let percent: u8 = text
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("`{text}` is neither a percent nor `auto`"))?;
    if !(1..=99).contains(&percent) {
        return Err(format!(
            "a frame phase of {percent}% lands on the SYNC0 edge; use 1..=99, or `auto`",
        ));
    }
    Ok(percent)
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
        Command::Tune { action } => match action {
            TuneAction::Run(args) => tune(&cli, &client, args)?,
            TuneAction::Status => emit(&cli, &client.tune_report()?, print_tune_report)?,
            TuneAction::Cancel => emit(&cli, &client.tune_cancel()?, |a| {
                println!("{}", a.message);
            })?,
            TuneAction::Apply { candidate } => {
                emit(&cli, &client.tune_apply(*candidate)?, |a| {
                    println!("{}", a.message);
                })?;
            }
        },
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

const TUNE_POLL: Duration = Duration::from_secs(2);

fn tune(cli: &Cli, client: &ApplianceClient, args: &TuneRunArgs) -> Result<()> {
    let request = TuneRequest {
        periods_ns: args.periods.iter().map(as_ns).collect(),
        frame_phase_percents: args.frame_phases.clone(),
        warmup_ns: as_ns(&args.warmup),
        dwell_ns: as_ns(&args.dwell),
        settle_ns: as_ns(&args.settle),
        poll_ns: as_ns(&args.poll),
    };
    let started = client.tune_start(&request)?;
    if args.detach {
        return emit(cli, &started, |a| println!("{}", a.message));
    }
    if !cli.json {
        let per = args.warmup + args.dwell + args.settle;
        println!(
            "{}; about {} at worst",
            started.message,
            humantime::format_duration(
                per * u32::try_from(request.periods_ns.len() * request.frame_phase_percents.len())
                    .unwrap_or(u32::MAX)
            ),
        );
    }

    let mut reported = 0usize;
    loop {
        std::thread::sleep(TUNE_POLL);
        let report = client.tune_report()?;
        if !cli.json {
            for candidate in report.candidates.iter().skip(reported) {
                println!("{}", tune_row(candidate));
            }
            reported = report.candidates.len();
        }
        if !report.running {
            return emit(cli, &report, print_tune_summary);
        }
    }
}

fn as_ns(duration: &Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn fmt_ns(ns: u64) -> String {
    humantime::format_duration(Duration::from_nanos(ns)).to_string()
}

fn fmt_frame_phase(candidate: &TuneCandidate) -> String {
    if candidate.target.is_auto() {
        "auto".to_owned()
    } else {
        format!(
            "{}% ({})",
            candidate.target.frame_phase_percent,
            fmt_ns(candidate.target.frame_phase_ns),
        )
    }
}

fn fmt_frames(candidate: &TuneCandidate) -> String {
    if !candidate.telemetry_read {
        return String::new();
    }
    format!(
        "delivered {:>6}  skipped {:>4}  ",
        candidate.frames_delivered, candidate.frames_skipped,
    )
}

fn tune_row(candidate: &TuneCandidate) -> String {
    format!(
        "{:>8}  {:>14}  {:>6.1}%  drops {:>3}  recov {:>3}  stale {:>4}  lost {:>4}  \
         phase-exc {:>5}  exchange {:>9}/{:<9}  {}{}{}",
        fmt_ns(candidate.target.period_ns),
        fmt_frame_phase(candidate),
        candidate.op_ratio() * 100.0,
        candidate.drop_events,
        candidate.recoveries,
        candidate.stale_cycles,
        candidate.lost_cycles,
        candidate.phase_excursions,
        fmt_ns(candidate.exchange_mean_ns),
        fmt_ns(candidate.exchange_worst_ns),
        fmt_frames(candidate),
        candidate.status.label(),
        candidate
            .note
            .as_ref()
            .map_or_else(String::new, |note| format!(" ({note})")),
    )
}

fn print_tune_report(report: &TuneReport) {
    for candidate in &report.candidates {
        println!("{}", tune_row(candidate));
    }
    print_tune_summary(report);
}

fn print_tune_summary(report: &TuneReport) {
    if let Some(error) = &report.error {
        println!("the sweep stopped: {error}");
    }
    if report.candidates.is_empty() {
        if report.running {
            println!("the sweep is running; no candidate has finished yet");
        } else if report.error.is_none() {
            println!("no sweep has run on this appliance");
        }
        return;
    }
    if report.cancelled {
        println!("the sweep was cancelled; the remaining candidates were not measured");
    }
    let Some(best) = report.best_candidate() else {
        if report
            .candidates
            .iter()
            .all(|c| c.status == TuneStatus::Infeasible)
        {
            println!(
                "every period was too short to carry one exchange; sweep longer periods, or \
                 drive fewer devices from this appliance",
            );
        } else {
            println!("no candidate held the bus long enough to recommend");
        }
        return;
    };
    println!(
        "\nbest: {} at {}, {:.1}% in OP over {} sample(s), {:.2}% of exchanges off their \
         landing phase",
        fmt_ns(best.target.period_ns),
        fmt_frame_phase(best),
        best.op_ratio() * 100.0,
        best.samples,
        best.per_exchange(best.phase_excursions) * 100.0,
    );
    println!("\n[bus]");
    println!("sync0_period = \"{}\"", fmt_ns(best.target.period_ns));
    println!(
        "frame_phase = \"{}\"",
        if best.target.is_auto() {
            "auto".to_owned()
        } else {
            fmt_ns(best.target.frame_phase_ns)
        },
    );
    println!("\nrun `tune apply` to write those two keys into the appliance configuration");
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
        "bus        {:?} (requested {:?}){}{}",
        bus.actual,
        bus.desired,
        if bus.has_unknown_state() {
            format!(" [{UNKNOWN_STATE_HINT}]")
        } else {
            String::new()
        },
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
