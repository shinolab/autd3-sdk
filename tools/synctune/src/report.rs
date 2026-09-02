use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use autd3_rs::protocol::TX_FRAME_BYTES;
use clap::ValueEnum;

use crate::cli::Common;
use crate::monitor::{CandidateResult, CandidateStatus};

fn micros(d: Duration) -> u128 {
    d.as_micros()
}

fn fmt_first_drop(d: Option<Duration>) -> String {
    d.map_or_else(
        || "-".to_string(),
        |d| format!("{:.0}ms", d.as_secs_f64() * 1e3),
    )
}

fn fmt_horizon(d: Option<Duration>) -> String {
    let Some(d) = d else {
        return "never (rate indistinguishable from zero)".to_string();
    };
    let secs = d.as_secs_f64();
    if secs < 120.0 {
        format!("{secs:.0}s")
    } else if secs < 7200.0 {
        format!("{:.1}min", secs / 60.0)
    } else if secs < 172_800.0 {
        format!("{:.1}h", secs / 3600.0)
    } else {
        format!("{:.1}d", secs / 86_400.0)
    }
}

const PHASE_LABEL: &str = "frame_phase";

pub fn print_drift(r: &CandidateResult) {
    let d = &r.drift;
    println!("\n=== synctune: drift (bus DC time vs host wall clock) ===");
    println!(
        "sync0_period : {}us\n{:<13}: {}us ({}% of period)",
        micros(r.period),
        PHASE_LABEL,
        micros(r.shift),
        r.shift_percent,
    );
    println!("status       : {}", r.status.label());
    if let Some(note) = &r.note {
        println!("note         : {note}");
    }
    if d.samples == 0 {
        let why = if r.status == CandidateStatus::Ok {
            "this link does not publish a DC system time"
        } else {
            "the run never got far enough to sample"
        };
        println!("samples      : 0 ({why})");
        return;
    }
    println!(
        "samples      : {} over {:.1}s",
        d.samples,
        d.window.as_secs_f64(),
    );
    println!(
        "offset       : first={:.3}ms last={:.3}ms (bus - host; the constant part is frame latency, not drift)",
        d.first_offset_ns / 1e6,
        d.last_offset_ns / 1e6,
    );
    match d.rate_ppm {
        Some(rate) => {
            println!(
                "rate         : {rate:+.3} ppm  ({:+.0} ns/s, regression residual {:.1}us rms)",
                rate * 1e3,
                d.residual_rms_ns / 1e3,
            );
            println!(
                "10ms apart in: {}\n 1s apart in : {}",
                fmt_horizon(d.time_to(Duration::from_millis(10))),
                fmt_horizon(d.time_to(Duration::from_secs(1))),
            );
        }
        None => println!("rate         : - (need at least two samples spanning some time)"),
    }
    println!(
        "OP retention : {:.2}%  ({}/{} samples all-OP)",
        r.op_ratio() * 100.0,
        r.op_all_samples,
        r.total_samples,
    );
    println!(
        "\nnote: the host clock is one half of this measurement — record whether \
NTP/chrony was\n      running (`timedatectl` on Linux, `w32tm /query /status` on Windows)."
    );
}

fn perftest_command(common: &Common, period_us: u128, shift_percent: u8) -> String {
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
        "cargo xtask tool perftest -- --link echocat --mode {mode} \
--sync0-period {period_us}us --shift-percent {shift_percent}{iface}{devices} --duration 60s"
    )
}

pub fn print_measure(r: &CandidateResult, common: &Common) {
    println!("\n=== synctune: measure ===");
    println!(
        "sync0_period : {}us\n{:<13}: {}us ({}% of period)",
        micros(r.period),
        PHASE_LABEL,
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
        "dc drift     : {}",
        r.drift.rate_ppm.map_or_else(
            || "- (no DC system time from this link)".to_string(),
            |rate| format!(
                "{rate:+.3} ppm over {:.1}s ({} samples); 10ms apart in {}",
                r.drift.window.as_secs_f64(),
                r.drift.samples,
                fmt_horizon(r.drift.time_to(Duration::from_millis(10))),
            ),
        ),
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
        PHASE_LABEL,
        PHASE_LABEL,
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
                "\nbest: sync0_period={}us  {}={}us ({}% of period)  ->  OP retention {:.2}%",
                micros(r.period),
                PHASE_LABEL,
                micros(r.shift),
                r.shift_percent,
                r.op_ratio() * 100.0,
            );
            println!(
                "  reproduce with: measure --sync0-period {}us --shift-percent {}",
                micros(r.period),
                r.shift_percent,
            );
            println!(
                "  load-test with: {}",
                perftest_command(common, micros(r.period), r.shift_percent),
            );
            println!(
                "  (tie-break: higher op_ratio, fewer drops, lower {PHASE_LABEL}, lower period)"
            );
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
recoveries,first_drop_ms,send_success,send_errors,throughput_fps,drift_samples,drift_window_s,\
drift_rate_ppm,drift_residual_rms_ns,note"
    )?;
    for r in results {
        let first_drop_ms = r
            .time_to_first_drop
            .map_or_else(String::new, |d| format!("{:.3}", d.as_secs_f64() * 1e3));
        let drift_rate_ppm = r
            .drift
            .rate_ppm
            .map_or_else(String::new, |rate| format!("{rate:.6}"));
        let note = r.note.as_deref().unwrap_or("").replace(',', ";");
        writeln!(
            f,
            "{},{},{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{:.3},{},{:.3},{},{:.1},{}",
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
            r.drift.samples,
            r.drift.window.as_secs_f64(),
            drift_rate_ppm,
            r.drift.residual_rms_ns,
            note,
        )?;
    }
    Ok(())
}
