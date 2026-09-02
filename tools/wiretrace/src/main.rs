use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use autd3_rs_wiretrace::protocol::Decoded;
use autd3_rs_wiretrace::replay::{DEFAULT_NUM_TRANSDUCERS, ReplayReport};
use autd3_rs_wiretrace::{capture, cycle, protocol, replay};

#[derive(Parser)]
#[command(
    name = "autd3-rs-wiretrace",
    about = "Replay and analyse an AUTD3 EtherCAT wire capture"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Summarise a capture: device count, cycles, period, anomalies")]
    Summary { capture: PathBuf },
    #[command(about = "Print the per-cycle sequence, command and acknowledgement timeline")]
    Decode {
        capture: PathBuf,
        #[arg(long, default_value_t = 0, help = "Start at this cycle index")]
        from: usize,
        #[arg(long, default_value_t = 64, help = "Print at most this many cycles")]
        limit: usize,
        #[arg(
            long,
            help = "Print every cycle instead of only the ones that look wrong"
        )]
        all: bool,
    },
    #[command(about = "Replay the captured tx into the firmware emulator and diff the rx")]
    Replay {
        capture: PathBuf,
        #[arg(long, default_value_t = DEFAULT_NUM_TRANSDUCERS, help = "Transducers per device")]
        transducers: usize,
        #[arg(
            long,
            default_value_t = 32,
            help = "Print at most this many disagreements"
        )]
        limit: usize,
    },
}

fn load(path: &PathBuf) -> Result<cycle::Trace> {
    let format = capture::format_of(path).with_context(|| format!("reading {}", path.display()))?;
    let frames = capture::read(path).with_context(|| format!("reading {}", path.display()))?;
    let trace = cycle::assemble(&frames).with_context(|| format!("parsing {}", path.display()))?;
    println!(
        "{}: {format:?}, {} frames, {} devices, {} cycles",
        path.display(),
        frames.len(),
        trace.num_devices,
        trace.cycles.len()
    );
    Ok(trace)
}

fn print_summary(trace: &cycle::Trace, decoded: &Decoded) {
    match trace.nominal_period_ns() {
        Some(period) => println!("  median cycle period: {} us", period / 1_000),
        None => println!("  median cycle period: unknown (fewer than two cycles)"),
    }
    println!("  acknowledgement lag: {} cycles", trace.ack_lag);
    let span = trace
        .cycles
        .last()
        .map_or(0, |last| last.timestamp_ns.saturating_sub(trace.started_ns));
    println!("  captured span: {} ms", span / 1_000_000);
    if trace.non_ethercat_frames > 0 {
        println!(
            "  skipped {} frames that were not EtherCAT",
            trace.non_ethercat_frames
        );
    }
    println!(
        "  cycles without a reply: {}",
        decoded.cycles_without_response
    );
    println!("  reset commands: {}", decoded.resets);
    if decoded.unknown_commands > 0 {
        println!("  unknown commands: {}", decoded.unknown_commands);
    }
    let incomplete = trace
        .cycles
        .iter()
        .filter(|record| !record.tx_complete())
        .count();
    if incomplete > 0 {
        println!("  cycles whose tx was only partly captured: {incomplete}");
    }
    if decoded.held_frames.is_empty() {
        println!("  frames staged for more than one cycle: none");
    } else {
        println!(
            "  frames staged for more than one cycle: {} (longest first)",
            decoded.held_frames.len()
        );
        for run in decoded.held_frames.iter().take(8) {
            println!(
                "    {} cycles ({}..={}) staged seq {} cmd {:#04x}",
                run.cycles(),
                run.from_cycle,
                run.to_cycle,
                run.seq,
                run.raw_cmd
            );
        }
    }
    if decoded.unacknowledged.is_empty() {
        println!("  frames never acknowledged: none");
    } else {
        println!(
            "  frames never acknowledged: {}",
            decoded.unacknowledged.len()
        );
        for entry in decoded.unacknowledged.iter().take(8) {
            println!(
                "    device {} never acknowledged seq {} cmd {:#04x} staged over cycles {}..={}",
                entry.device, entry.seq, entry.raw_cmd, entry.from_cycle, entry.to_cycle
            );
        }
    }
}

fn print_decode(decoded: &Decoded, from: usize, limit: usize, all: bool) {
    println!("  cycle      time_us  seq  cmd                  acks              flags");
    let mut shown = 0usize;
    for cycle in decoded.cycles.iter().skip(from) {
        if shown >= limit {
            println!("  ... stopped after {limit} cycles (raise --limit to see more)");
            break;
        }
        let healthy = cycle.responded && cycle.rx_valid && cycle.staged_uniformly;
        if !all && healthy && cycle.acked_by_every_device() {
            continue;
        }
        let cmd = cycle.cmd.map_or_else(
            || format!("{:#04x}?", cycle.raw_cmd),
            |cmd| format!("{cmd:?}"),
        );
        let acks = if cycle.acks.is_empty() {
            "?".to_owned()
        } else {
            cycle
                .acks
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut flags = Vec::new();
        if !cycle.responded {
            flags.push("no-reply");
        }
        if !cycle.rx_valid {
            flags.push("bad-wkc");
        }
        if !cycle.staged_uniformly {
            flags.push("mixed-tx");
        }
        println!(
            "  {:5}  {:11}  {:3}  {cmd:<19}  {acks:<16}  {}",
            cycle.index,
            cycle.timestamp_ns / 1_000,
            cycle.seq,
            flags.join(" ")
        );
        shown += 1;
    }
    if shown == 0 {
        println!("  every cycle looks healthy (pass --all to print them anyway)");
    }
}

fn describe_command(raw_cmd: u8) -> String {
    autd3_cpu_wire::Cmd::from_u8(raw_cmd)
        .map_or_else(|| format!("{raw_cmd:#04x}"), |cmd| format!("{cmd:?}"))
}

fn print_replay(report: &ReplayReport, limit: usize) {
    println!(
        "  acknowledgement lag measured from the capture: {} cycles",
        report.ack_lag
    );
    println!("  cycles fed to the emulator: {}", report.cycles_fed);
    println!(
        "  cycles skipped because delivery was never confirmed: {}",
        report.cycles_unconfirmed
    );
    println!("  cycles compared: {}", report.cycles_compared);
    if report.cycles_with_incomplete_tx > 0 {
        println!(
            "  cycles fed with a partly captured tx: {}",
            report.cycles_with_incomplete_tx
        );
    }
    match report.converged_at {
        Some(0) | None => {}
        Some(reset) => println!(
            "  the capture starts mid-session, so the emulator only matches from the first reset \
             at cycle {reset} ({} earlier disagreements ignored)",
            report.diffs_before_convergence
        ),
    }
    if report.agrees() {
        println!("  the emulator reproduced every acknowledgement");
        return;
    }
    println!("  disagreements: {}", report.diffs.len());
    let mut by_command: Vec<(u8, usize)> = Vec::new();
    for diff in &report.diffs {
        match by_command.iter_mut().find(|(cmd, _)| *cmd == diff.raw_cmd) {
            Some((_, count)) => *count += 1,
            None => by_command.push((diff.raw_cmd, 1)),
        }
    }
    by_command.sort_unstable_by_key(|(cmd, count)| (std::cmp::Reverse(*count), *cmd));
    for (raw_cmd, count) in &by_command {
        println!("    {count} on {}", describe_command(*raw_cmd));
    }
    for diff in report.diffs.iter().take(limit) {
        let cmd = describe_command(diff.raw_cmd);
        println!(
            "    cycle {} device {} ({cmd}): captured ack {} data {:#04x}, replayed ack {} data {:#04x}",
            diff.cycle,
            diff.device,
            diff.captured[0],
            diff.captured[1],
            diff.replayed[0],
            diff.replayed[1]
        );
    }
    if report.diffs.len() > limit {
        println!(
            "    ... and {} more (raise --limit to see them)",
            report.diffs.len() - limit
        );
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Summary { capture } => {
            let trace = load(&capture)?;
            let decoded = protocol::decode(&trace);
            print_summary(&trace, &decoded);
        }
        Command::Decode {
            capture,
            from,
            limit,
            all,
        } => {
            let trace = load(&capture)?;
            let decoded = protocol::decode(&trace);
            print_decode(&decoded, from, limit, all);
        }
        Command::Replay {
            capture,
            transducers,
            limit,
        } => {
            let trace = load(&capture)?;
            let (_, report) = replay::replay(&trace, transducers);
            print_replay(&report, limit);
        }
    }
    Ok(())
}
