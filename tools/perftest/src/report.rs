use std::time::Duration;

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};

use autd3_rs::Telemetry;

use crate::mem::MemProfile;
use crate::stats::{Counters, LatencyStats, Summary};

struct TelemetryCounter {
    name: &'static str,
    read: fn(&Telemetry) -> u32,
    mask: u32,
    per_device: bool,
}

const TELEMETRY_COUNTERS: [TelemetryCounter; 10] = [
    TelemetryCounter {
        name: "fifo_drop",
        read: |t| t.fifo_drop,
        mask: u32::MAX,
        per_device: true,
    },
    TelemetryCounter {
        name: "dedup",
        read: |t| t.dedup,
        mask: u32::MAX,
        per_device: true,
    },
    TelemetryCounter {
        name: "seq_mismatch",
        read: |t| t.seq_mismatch,
        mask: u32::MAX,
        per_device: true,
    },
    TelemetryCounter {
        name: "dispatch_error",
        read: |t| t.dispatch_error,
        mask: u32::MAX,
        per_device: true,
    },
    TelemetryCounter {
        name: "send_failure",
        read: |t| t.send_failure,
        mask: u32::MAX,
        per_device: true,
    },
    TelemetryCounter {
        name: "failsafe",
        read: |t| u32::from(t.failsafe),
        mask: 0xFFFF,
        per_device: true,
    },
    TelemetryCounter {
        name: "ptp_unlock_failsafe",
        read: |t| u32::from(t.ptp_unlock_failsafe),
        mask: 0xFFFF,
        per_device: true,
    },
    TelemetryCounter {
        name: "sync_resync",
        read: |t| u32::from(t.sync_resync),
        mask: 0xFF,
        per_device: true,
    },
    TelemetryCounter {
        name: "boot_failure",
        read: |t| u32::from(t.boot_failure),
        mask: 0xFF,
        per_device: true,
    },
    TelemetryCounter {
        name: "processed",
        read: |t| t.processed,
        mask: u32::MAX,
        per_device: false,
    },
];

impl TelemetryCounter {
    fn deltas(&self, start: &[Telemetry], now: &[Telemetry]) -> Vec<u32> {
        start
            .iter()
            .zip(now)
            .map(|(s, n)| (self.read)(n).wrapping_sub((self.read)(s)) & self.mask)
            .collect()
    }
}

pub struct LiveBlock {
    terminal: bool,
    drawn_lines: usize,
}

impl LiveBlock {
    pub fn new() -> Self {
        Self {
            terminal: std::io::stdout().is_terminal(),
            drawn_lines: 0,
        }
    }

    pub fn draw(&mut self, lines: &[String]) {
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

pub fn interval_lines(
    elapsed: Duration,
    cmd_per_sec: f64,
    counters: &Counters,
    missed_replies: u64,
    latency: &LatencyStats,
) -> [String; 2] {
    [
        format!(
            "[{:>8.1}s] {cmd_per_sec:>10.1} cmd/s  ok {}  timeout {}  dev_err {}  net_err {}  missed {missed_replies}",
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
    ]
}

pub fn telemetry_lines(start: &[Telemetry], now: &[Telemetry]) -> Vec<String> {
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

pub fn print_telemetry(start: &[Telemetry], end: &[Telemetry]) {
    println!();
    println!("telemetry (per device, at start -> delta over the run):");
    for counter in &TELEMETRY_COUNTERS {
        let at_start: Vec<u32> = start.iter().map(counter.read).collect();
        println!(
            "  {:<20}: {at_start:?} -> {:?}",
            counter.name,
            counter.deltas(start, end)
        );
    }
}

pub fn print_summary(s: &Summary) {
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
    println!("  missed replies: {}", s.missed_replies);

    println!();
    println!("throughput      :");
    println!("  cmd / sec     : {:>12.2}", s.throughput_cmd_per_sec);
    println!("  byte / sec    : {:>12.2}", s.throughput_byte_per_sec);

    println!();
    println!("latency (successful samples):");
    println!("  mean          : {}", format_duration(s.latency.mean));
    println!("  p50           : {}", format_duration(s.latency.p50));
    println!("  p95           : {}", format_duration(s.latency.p95));
    println!("  p99           : {}", format_duration(s.latency.p99));
    println!("  min           : {}", format_duration(s.latency.min));
    println!("  max           : {}", format_duration(s.latency.max));

    if let Some(ack) = &s.driver_ack {
        println!();
        println!("driver ack latency (per frame, send -> acked by all devices):");
        println!("  frames        : {}", ack.frames);
        println!("  mean          : {}", format_duration(ack.mean));
        println!(
            "  worst         : {}   (since open, start-up included)",
            format_duration(ack.worst)
        );
    }
}

pub fn print_mem(m: &MemProfile) {
    let per_send = |n: u64| {
        if m.sends == 0 {
            0.0
        } else {
            n as f64 / m.sends as f64
        }
    };
    let net_bytes = i128::from(m.bytes_allocated) - i128::from(m.bytes_deallocated);

    println!();
    println!("memory (mem-profile build, process-wide):");
    println!(
        "  allocations   : {:>12}   (per send: {:.2})",
        m.allocations,
        per_send(m.allocations),
    );
    println!("  deallocations : {:>12}", m.deallocations);
    println!("  reallocations : {:>12}", m.reallocations);
    println!(
        "  bytes alloc   : {:>12}   (per send: {:.2})",
        m.bytes_allocated,
        per_send(m.bytes_allocated),
    );
    println!("  net bytes     : {net_bytes:>12}");

    if m.top_sizes.is_empty() && m.large_count == 0 {
        return;
    }
    println!();
    println!("  allocation sizes (largest share of bytes first):");
    println!(
        "    {:>8}  {:>10}  {:>10}  {:>12}",
        "size", "count", "per send", "bytes/send"
    );
    for bucket in &m.top_sizes {
        let bytes = bucket.size as u64 * bucket.count;
        println!(
            "    {:>8}  {:>10}  {:>10.2}  {:>12.2}",
            bucket.size,
            bucket.count,
            per_send(bucket.count),
            per_send(bytes),
        );
    }
    if m.large_count > 0 {
        println!(
            "    {:>8}  {:>10}  {:>10.2}  {:>12.2}",
            ">=8192",
            m.large_count,
            per_send(m.large_count),
            per_send(m.large_bytes),
        );
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
