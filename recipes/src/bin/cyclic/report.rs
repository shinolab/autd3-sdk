use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};
use std::time::Duration;

use autd3_cpu_wire::udp::{FrameReply, Header};
use autd3_rs::Telemetry;
use autd3_rs::protocol::FrameHeader;
use autd3_rs::value::SysTime;
use hdrhistogram::Histogram;

use crate::controller::CycleReport;

pub(crate) struct Stats {
    num_devices: usize,
    ok: u64,
    missing: u64,
    device_errors: u64,
    sent_datagrams: u64,
    sent_bytes: u64,
    received_datagrams: u64,
    send_late: Histogram<u64>,
    done: Histogram<u64>,
    spread: Histogram<u64>,
    margin: Histogram<u64>,
}

fn histogram() -> Histogram<u64> {
    Histogram::new_with_max(60_000_000_000, 3).expect("valid histogram bounds")
}

fn signed_ns(t: u64) -> i64 {
    i64::try_from(t).unwrap_or(i64::MAX)
}

fn record(histogram: &mut Histogram<u64>, ns: i64) {
    histogram.saturating_record(u64::try_from(ns).unwrap_or(0));
}

impl Stats {
    pub(crate) fn new(num_devices: usize) -> Self {
        Self {
            num_devices,
            ok: 0,
            missing: 0,
            device_errors: 0,
            sent_datagrams: 0,
            sent_bytes: 0,
            received_datagrams: 0,
            send_late: histogram(),
            done: histogram(),
            spread: histogram(),
            margin: histogram(),
        }
    }

    pub(crate) fn record(&mut self, report: &CycleReport) {
        if report.replies.len() < self.num_devices {
            self.missing += 1;
        } else if report.replies.iter().any(|reply| reply.status != 0) {
            self.device_errors += 1;
        } else {
            self.ok += 1;
        }
        self.sent_datagrams += self.num_devices as u64;
        self.sent_bytes += (self.num_devices * (size_of::<Header>() + size_of::<FrameHeader>())
            + report.payload_bytes) as u64;
        self.received_datagrams += report.replies.len() as u64;

        let ns = |t: SysTime| signed_ns(t.sys_time());
        record(
            &mut self.send_late,
            ns(report.sent_at) - ns(report.send_due),
        );
        let exec_at = ns(report.exec_at);
        let done: Vec<i64> = report
            .replies
            .iter()
            .filter(|reply| reply.sys_time != 0)
            .map(|reply| signed_ns(reply.sys_time))
            .collect();
        for &t in &done {
            record(&mut self.done, t - exec_at);
        }
        if let (Some(first), Some(last)) = (done.iter().min(), done.iter().max()) {
            record(&mut self.spread, last - first);
        }
        for reply in report.replies.iter().filter(|reply| reply.rx_sys_time != 0) {
            record(&mut self.margin, exec_at - signed_ns(reply.rx_sys_time));
        }
    }

    fn cycles(&self) -> u64 {
        self.ok + self.missing + self.device_errors
    }

    fn line_percent(datagrams: u64, bytes: u64, secs: f64) -> f64 {
        (bytes + datagrams * (8 + 40 + 14 + 4 + 8 + 12)) as f64 * 8.0 / secs / 100_000_000.0 * 100.0
    }

    fn line_down_percent(&self, secs: f64) -> f64 {
        Self::line_percent(self.sent_datagrams, self.sent_bytes, secs)
    }

    fn line_up_percent(&self, secs: f64) -> f64 {
        let reply_bytes =
            self.received_datagrams * (size_of::<Header>() + size_of::<FrameReply>()) as u64;
        Self::line_percent(self.received_datagrams, reply_bytes, secs)
    }
}

pub(crate) struct TelemetryCounters {
    start: Vec<Option<Telemetry>>,
    now: Vec<Option<Telemetry>>,
}

struct TelemetryCounter {
    name: &'static str,
    read: fn(&Telemetry) -> u32,
    mask: u32,
    absolute: bool,
}

const TELEMETRY_COUNTERS: [TelemetryCounter; 5] = [
    TelemetryCounter {
        name: "fifo_drop",
        read: |t| t.fifo_drop,
        mask: u32::MAX,
        absolute: false,
    },
    TelemetryCounter {
        name: "exec_late",
        read: |t| t.exec_late,
        mask: u32::MAX,
        absolute: false,
    },
    TelemetryCounter {
        name: "sync_resync",
        read: |t| u32::from(t.sync_resync),
        mask: 0xFF,
        absolute: true,
    },
    TelemetryCounter {
        name: "cpu_watchdog",
        read: |t| u32::from(t.cpu_watchdog),
        mask: 0xFF,
        absolute: false,
    },
    TelemetryCounter {
        name: "boot_failure",
        read: |t| u32::from(t.boot_failure),
        mask: 0xFF,
        absolute: false,
    },
];

impl TelemetryCounters {
    pub(crate) fn new(num_devices: usize) -> Self {
        Self {
            start: vec![None; num_devices],
            now: vec![None; num_devices],
        }
    }

    pub(crate) fn record(&mut self, report: &CycleReport) {
        for reply in &report.replies {
            self.start[reply.device].get_or_insert(reply.telemetry);
            self.now[reply.device] = Some(reply.telemetry);
        }
    }

    fn deltas(&self, counter: &TelemetryCounter) -> Vec<u32> {
        self.start
            .iter()
            .zip(&self.now)
            .map(|(start, now)| match (start, now) {
                (_, Some(now)) if counter.absolute => (counter.read)(now),
                (Some(start), Some(now)) => {
                    (counter.read)(now).wrapping_sub((counter.read)(start)) & counter.mask
                }
                _ => 0,
            })
            .collect()
    }

    fn at_start(&self, counter: &TelemetryCounter) -> Vec<u32> {
        self.start
            .iter()
            .map(|start| start.as_ref().map_or(0, counter.read))
            .collect()
    }

    fn lines(&self) -> Vec<String> {
        TELEMETRY_COUNTERS
            .chunks(TELEMETRY_COUNTERS.len().div_ceil(2))
            .enumerate()
            .map(|(row, counters)| {
                let mut line = format!("{:>11}{:<9}", "", if row == 0 { "telemetry" } else { "" });
                for counter in counters {
                    let deltas = self.deltas(counter);
                    let total: u64 = deltas.iter().map(|&d| u64::from(d)).sum();
                    let _ = write!(line, "  {} {total}", counter.name);
                    if total != 0 && deltas.len() > 1 {
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

    pub(crate) fn draw(
        &mut self,
        elapsed: Duration,
        interval: Duration,
        stats: &Stats,
        total: &Stats,
        telemetry: &TelemetryCounters,
    ) {
        let secs = interval.as_secs_f64();
        let latency = |h: &Histogram<u64>| {
            format!(
                "mean {}  p50 {}  p99 {}  max {}",
                format_mean(h),
                format_ns(h.value_at_quantile(0.5)),
                format_ns(h.value_at_quantile(0.99)),
                format_ns(h.max()),
            )
        };
        let mut lines = vec![
            format!(
                "[{:>8.1}s] {:>10.1} cycle/s  ok {}  missing {}  dev_err {}",
                elapsed.as_secs_f64(),
                stats.cycles() as f64 / secs,
                total.ok,
                total.missing,
                total.device_errors,
            ),
            format!("{:>11}send late  {}", "", latency(&stats.send_late)),
            format!("{:>11}done       {}", "", latency(&stats.done)),
            format!("{:>11}spread     {}", "", latency(&stats.spread)),
            format!(
                "{:>11}margin     min {}  p1 {}  p50 {}",
                "",
                format_ns(stats.margin.min()),
                format_ns(stats.margin.value_at_quantile(0.01)),
                format_ns(stats.margin.value_at_quantile(0.5)),
            ),
            format!(
                "{:>11}line       sent {:.0} byte/s  down {:.1} %  up {:.1} %",
                "",
                stats.sent_bytes as f64 / secs,
                stats.line_down_percent(secs),
                stats.line_up_percent(secs),
            ),
        ];
        lines.extend(telemetry.lines());

        let mut block = String::new();
        if self.terminal {
            if self.drawn_lines > 0 {
                let _ = write!(block, "\x1b[{}A", self.drawn_lines);
            }
            block.push_str("\r\x1b[J\x1b[?7l");
        }
        for line in &lines {
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

fn print_latency(title: &str, h: &Histogram<u64>) {
    println!();
    println!("{title}:");
    println!("  mean          : {}", format_mean(h));
    for (label, quantile) in [("p50", 0.5), ("p95", 0.95), ("p99", 0.99)] {
        println!(
            "  {label:<14}: {}",
            format_ns(h.value_at_quantile(quantile))
        );
    }
    println!("  min           : {}", format_ns(h.min()));
    println!("  max           : {}", format_ns(h.max()));
}

pub(crate) fn print_summary(
    elapsed: Duration,
    period: Duration,
    lead: Duration,
    s: &Stats,
    telemetry: &TelemetryCounters,
) {
    let secs = elapsed.as_secs_f64();
    println!();
    println!("=== cyclic summary ===");
    println!("elapsed         : {}", format_duration(elapsed));
    println!("period          : {}", format_duration(period));
    println!("lead            : {}", format_duration(lead));
    println!("cycles          : {}", s.cycles());
    println!("  success       : {}", s.ok);
    println!("  missing       : {}   (a device did not reply)", s.missing);
    println!("  device errors : {}", s.device_errors);

    println!();
    println!("throughput      :");
    println!("  cycle / sec   : {:>12.2}", s.cycles() as f64 / secs);
    println!(
        "  byte / sec    : {:>12.2}   (UDP payload sent, all devices)",
        s.sent_bytes as f64 / secs
    );
    println!(
        "  line down     : {:>12.1} %   (host -> devices, of 100 Mbps, headers and gaps included)",
        s.line_down_percent(secs)
    );
    println!(
        "  line up       : {:>12.1} %   (replies)",
        s.line_up_percent(secs)
    );

    print_latency(
        "send late (host, device time at send_frame - EXEC_AT + lead)",
        &s.send_late,
    );
    print_latency(
        "done (per device, device time of the reply - EXEC_AT)",
        &s.done,
    );
    print_latency(
        "spread (per cycle, last device done - first device done)",
        &s.spread,
    );
    print_latency(
        "margin (per device, EXEC_AT - device time the frame arrived; 0 = arrived late)",
        &s.margin,
    );

    println!();
    println!(
        "telemetry (per device, at the first reply -> delta over the run; sync_resync: at the first reply -> at the end):"
    );
    for counter in &TELEMETRY_COUNTERS {
        println!(
            "  {:<20}: {:?} -> {:?}",
            counter.name,
            telemetry.at_start(counter),
            telemetry.deltas(counter)
        );
    }
}

fn format_mean(h: &Histogram<u64>) -> String {
    format_duration(Duration::from_secs_f64(h.mean() / 1e9))
}

fn format_ns(ns: u64) -> String {
    format_duration(Duration::from_nanos(ns))
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
