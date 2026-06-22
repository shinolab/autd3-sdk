use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use autd3_rs::TX_FRAME_BYTES;
use clap::ValueEnum;

use crate::cli::Common;
use crate::monitor::CandidateResult;

fn micros(d: Duration) -> u128 {
    d.as_micros()
}

fn fmt_first_drop(d: Option<Duration>) -> String {
    d.map_or_else(
        || "-".to_string(),
        |d| format!("{:.0}ms", d.as_secs_f64() * 1e3),
    )
}

fn perftest_command(common: &Common, period_us: u128, shift_percent: u8) -> String {
    let link = common
        .link
        .to_possible_value()
        .map_or_else(|| "ethercrab".to_string(), |v| v.get_name().to_string());
    let mode = common
        .mode
        .to_possible_value()
        .map_or_else(|| "streaming".to_string(), |v| v.get_name().to_string());
    let iface = common
        .interface
        .as_ref()
        .map_or_else(String::new, |i| format!(" --interface {i}"));
    let devices = common
        .devices
        .map_or_else(String::new, |d| format!(" --devices {d}"));
    format!(
        "cargo xtask tool perftest -- --link {link} --mode {mode} \
--cycle-us {period_us} --shift-percent {shift_percent}{iface}{devices} --duration 60s"
    )
}

pub fn print_measure(r: &CandidateResult, common: &Common) {
    println!("\n=== synctune: measure ===");
    println!(
        "sync0_period : {}us\nsync0_shift  : {}us ({}% of period)",
        micros(r.period),
        micros(r.shift),
        r.shift_percent,
    );
    println!("status       : {}", r.status.label());
    if let Some(note) = &r.note {
        println!("note         : {note}");
    }
    println!(
        "OP retention : {:.2}%  ({}/{} samples all-OP)",
        r.op_ratio() * 100.0,
        r.op_all_samples,
        r.total_samples,
    );
    println!(
        "degraded     : safe-op={} safe-op-err={} lost={} other={}",
        r.safe_op_samples, r.safe_op_error_samples, r.lost_samples, r.other_samples,
    );
    println!(
        "events       : drops={} lost={} recoveries={} first-drop={}",
        r.drop_events,
        r.lost_events,
        r.recoveries,
        fmt_first_drop(r.time_to_first_drop),
    );
    println!(
        "load (xorhash): success={} errors={}",
        r.send_success, r.send_errors,
    );
    println!(
        "throughput   : {:.0} frames/s  ({:.2} MB/s, window {:.1}s)",
        r.load.throughput_fps(),
        r.load.throughput_fps() * TX_FRAME_BYTES as f64 / 1e6,
        r.load.window.as_secs_f64(),
    );
    println!(
        "\nload-test with perftest:\n  {}",
        perftest_command(common, micros(r.period), r.shift_percent),
    );
}

pub fn print_table(results: &[CandidateResult], best: Option<usize>) {
    println!("\n=== synctune: tune results ===");
    println!(
        "{:<3} {:>9} {:>8} {:>8} {:>11} {:>9} {:>5} {:>5} {:>6} {:>10} {:>9}",
        "",
        "period",
        "shift",
        "shift",
        "status",
        "op_ret",
        "drop",
        "lost",
        "recov",
        "first-drop",
        "throughput",
    );
    println!(
        "{:<3} {:>9} {:>8} {:>8} {:>11} {:>9} {:>5} {:>5} {:>6} {:>10} {:>9}",
        "", "[us]", "[us]", "[%]", "", "[%]", "", "", "", "", "[fps]",
    );
    for (i, r) in results.iter().enumerate() {
        let marker = if Some(i) == best { "*" } else { " " };
        println!(
            "{:<3} {:>9} {:>8} {:>8} {:>11} {:>9.2} {:>5} {:>5} {:>6} {:>10} {:>9.0}",
            marker,
            micros(r.period),
            micros(r.shift),
            r.shift_percent,
            r.status.label(),
            r.op_ratio() * 100.0,
            r.drop_events,
            r.lost_events,
            r.recoveries,
            fmt_first_drop(r.time_to_first_drop),
            r.load.throughput_fps(),
        );
    }
}

pub fn print_best(results: &[CandidateResult], best: Option<usize>, common: &Common) {
    match best {
        Some(i) => {
            let r = &results[i];
            println!(
                "\nbest: sync0_period={}us  sync0_shift={}us ({}% of period)  ->  OP retention {:.2}%",
                micros(r.period),
                micros(r.shift),
                r.shift_percent,
                r.op_ratio() * 100.0,
            );
            println!(
                "  reproduce with: measure --cycle-us {} --shift-percent {}",
                micros(r.period),
                r.shift_percent,
            );
            println!(
                "  load-test with: {}",
                perftest_command(common, micros(r.period), r.shift_percent),
            );
            println!("  (tie-break: higher op_ratio, fewer drops, lower shift, lower period)");
        }
        None => println!("\nbest: none (no candidate produced measurable samples)"),
    }
}

pub fn write_csv(path: &Path, results: &[CandidateResult]) -> io::Result<()> {
    let mut f = File::create(path)?;
    writeln!(
        f,
        "period_us,shift_us,shift_percent,status,op_ratio,total_samples,op_all_samples,\
safe_op_samples,safe_op_error_samples,lost_samples,other_samples,drop_events,lost_events,\
recoveries,first_drop_ms,send_success,send_errors,throughput_fps,note"
    )?;
    for r in results {
        let first_drop_ms = r
            .time_to_first_drop
            .map_or_else(String::new, |d| format!("{:.3}", d.as_secs_f64() * 1e3));
        let note = r.note.as_deref().unwrap_or("").replace(',', ";");
        writeln!(
            f,
            "{},{},{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{:.3},{}",
            micros(r.period),
            micros(r.shift),
            r.shift_percent,
            r.status.label(),
            r.op_ratio(),
            r.total_samples,
            r.op_all_samples,
            r.safe_op_samples,
            r.safe_op_error_samples,
            r.lost_samples,
            r.other_samples,
            r.drop_events,
            r.lost_events,
            r.recoveries,
            first_drop_ms,
            r.send_success,
            r.send_errors,
            r.load.throughput_fps(),
            note,
        )?;
    }
    Ok(())
}
