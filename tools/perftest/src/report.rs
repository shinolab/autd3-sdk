use std::time::Duration;

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};

use autd3_rs::Telemetry;

use crate::stats::{Counters, LatencyStats, Summary, TrafficRate};

struct TelemetryCounter {
    name: &'static str,
    read: fn(&Telemetry) -> u32,
    mask: u32,
    per_device: bool,
    absolute: bool,
}

const TELEMETRY_COUNTERS: [TelemetryCounter; 9] = [
    TelemetryCounter {
        name: "fifo_drop",
        read: |t| t.fifo_drop,
        mask: u32::MAX,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "dispatch_error",
        read: |t| t.dispatch_error,
        mask: u32::MAX,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "send_failure",
        read: |t| t.send_failure,
        mask: u32::MAX,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "failsafe",
        read: |t| u32::from(t.failsafe),
        mask: 0xFFFF,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "ptp_unlock_failsafe",
        read: |t| u32::from(t.ptp_unlock_failsafe),
        mask: 0xFFFF,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "sync_resync",
        read: |t| u32::from(t.sync_resync),
        mask: 0xFF,
        per_device: true,
        absolute: true,
    },
    TelemetryCounter {
        name: "boot_failure",
        read: |t| u32::from(t.boot_failure),
        mask: 0xFF,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "exec_late",
        read: |t| t.exec_late,
        mask: u32::MAX,
        per_device: true,
        absolute: false,
    },
    TelemetryCounter {
        name: "processed",
        read: |t| t.processed,
        mask: u32::MAX,
        per_device: false,
        absolute: false,
    },
];

impl TelemetryCounter {
    fn deltas(&self, start: &[Telemetry], now: &[Telemetry]) -> Vec<u32> {
        if self.absolute {
            return now.iter().map(self.read).collect();
        }
        start
            .iter()
            .zip(now)
            .map(|(s, n)| (self.read)(n).wrapping_sub((self.read)(s)) & self.mask)
            .collect()
    }
}

pub(crate) struct LiveBlock {
    terminal: bool,
    drawn_lines: usize,
}

impl LiveBlock {
    pub(crate) fn new() -> Self {
        Self {
            terminal: std::io::stdout().is_terminal(),
            drawn_lines: 0,
        }
    }

    pub(crate) fn draw(&mut self, lines: &[String]) {
        let mut block = String::new();
        if self.terminal {
            if self.drawn_lines > 0 {
                let _ = write!(block, "\x1b[{}A", self.drawn_lines);
            }
            block.push_str("\r\x1b[J\x1b[?7l");
        }
        for line in lines {
            block.push_str(line);
            block.push('\n');
        }
        if self.terminal {
            block.push_str("\x1b[?7h");
        }
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(block.as_bytes());
        let _ = stdout.flush();
        self.drawn_lines = lines.len();
    }
}

pub(crate) fn interval_lines(
    elapsed: Duration,
    cmd_per_sec: f64,
    counters: &Counters,
    latency: &LatencyStats,
    traffic: &TrafficRate,
) -> [String; 3] {
    [
        format!(
            "[{:>8.1}s] {cmd_per_sec:>10.1} cmd/s  ok {}  timeout {}  dev_err {}  net_err {}",
            elapsed.as_secs_f64(),
            counters.ok,
            counters.timeouts,
            counters.device_error_total(),
            counters.network_errors,
        ),
        format!(
            "{:>11}rtt        mean {}  p50 {}  p99 {}  max {}",
            "",
            format_duration(latency.mean),
            format_duration(latency.p50),
            format_duration(latency.p99),
            format_duration(latency.max),
        ),
        format!(
            "{:>11}line       sent {:.0} byte/s  down {:.1} %  up {:.1} %",
            "", traffic.tx_byte_per_sec, traffic.line_down_percent, traffic.line_up_percent,
        ),
    ]
}

pub(crate) fn telemetry_lines(start: &[Telemetry], now: &[Telemetry]) -> Vec<String> {
    TELEMETRY_COUNTERS
        .chunks(TELEMETRY_COUNTERS.len().div_ceil(2))
        .enumerate()
        .map(|(row, counters)| {
            let mut line = format!("{:>11}{:<9}", "", if row == 0 { "telemetry" } else { "" });
            for counter in counters {
                let deltas = counter.deltas(start, now);
                let total: u64 = deltas.iter().map(|&d| u64::from(d)).sum();
                let _ = write!(line, "  {} {total}", counter.name);
                if total != 0 && counter.per_device && deltas.len() > 1 {
                    let devices: Vec<String> = deltas
                        .iter()
                        .enumerate()
                        .filter(|&(_, &d)| d != 0)
                        .map(|(i, d)| format!("dev{i}: {d}"))
                        .collect();
                    let _ = write!(line, " [{}]", devices.join(", "));
                }
            }
            line
        })
        .collect()
}

pub(crate) fn print_telemetry(start: &[Telemetry], end: &[Telemetry]) {
    println!();
    println!(
        "telemetry (per device, at start -> delta over the run; sync_resync: at start -> at end, cleared by Synchronize):"
    );
    for counter in &TELEMETRY_COUNTERS {
        let at_start: Vec<u32> = start.iter().map(counter.read).collect();
        println!(
            "  {:<20}: {at_start:?} -> {:?}",
            counter.name,
            counter.deltas(start, end)
        );
    }
}

pub(crate) fn print_summary(s: &Summary) {
    let c = &s.counters;

    println!();
    println!("=== perftest summary ===");
    println!("elapsed         : {}", format_duration(s.elapsed));
    println!("total sends     : {}", c.total());
    println!("  success       : {}", c.ok);
    println!("  timeouts      : {}", c.timeouts);
    println!("  network errors: {}", c.network_errors);
    if c.device_errors.is_empty() {
        println!("  device errors : 0");
    } else {
        println!("  device errors :");
        for (code, count) in &c.device_errors {
            println!("    code {code:#04x} : {count}");
        }
    }

    println!();
    println!("throughput      :");
    println!("  cmd / sec     : {:>12.2}", s.throughput_cmd_per_sec);
    println!(
        "  byte / sec    : {:>12.2}   (UDP payload sent, all devices)",
        s.traffic.tx_byte_per_sec
    );
    println!(
        "  line down     : {:>12.1} %   (host -> devices, of 100 Mbps, headers and gaps included)",
        s.traffic.line_down_percent
    );
    println!(
        "  line up       : {:>12.1} %   (replies)",
        s.traffic.line_up_percent
    );

    println!();
    println!("latency (successful samples):");
    println!("  mean          : {}", format_duration(s.latency.mean));
    println!("  p50           : {}", format_duration(s.latency.p50));
    println!("  p95           : {}", format_duration(s.latency.p95));
    println!("  p99           : {}", format_duration(s.latency.p99));
    println!("  min           : {}", format_duration(s.latency.min));
    println!("  max           : {}", format_duration(s.latency.max));

    println!();
    println!("frames:");
    println!("  sent          : {:>12}", s.frames);
    println!("  heartbeats    : {:>12}   (sent by --hold)", s.heartbeats);

    if let Some(mean) = s.ack.mean() {
        println!();
        println!(
            "ack latency (per frame, send_frame -> ResponseFuture resolved, wake-up included):"
        );
        println!("  frames        : {}", s.ack.frames());
        println!("  mean          : {}", format_duration(mean));
        println!("  max           : {}", format_duration(s.ack.max()));
    }
}

fn format_duration(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.3} us", d.as_secs_f64() * 1e6)
    } else if ns < 1_000_000_000 {
        format!("{:.3} ms", d.as_secs_f64() * 1e3)
    } else {
        format!("{:.3} s", d.as_secs_f64())
    }
}
