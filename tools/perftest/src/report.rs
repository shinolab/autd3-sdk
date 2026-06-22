use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Duration;

use crate::mem::MemProfile;
use crate::stats::{Sample, SampleStatus, Summary};

pub fn print_summary(s: &Summary) {
    let total = s.success + s.timeouts + s.link_errors + s.device_errors.values().sum::<u64>();

    println!("=== perftest summary ===");
    println!("elapsed         : {}", format_duration(s.elapsed));
    println!("total sends     : {total}");
    println!("  success       : {}", s.success);
    println!("  timeouts      : {}", s.timeouts);
    println!("  link errors   : {}", s.link_errors);
    if s.device_errors.is_empty() {
        println!("  device errors : 0");
    } else {
        println!("  device errors :");
        for (code, count) in &s.device_errors {
            println!("    code {code:#04x} : {count}");
        }
    }
    println!("  stale cycles  : {}", s.stale_cycles);
    println!("  lost cycles   : {}", s.lost_cycles);

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
}

pub fn write_csv(path: &Path, samples: &[Sample]) -> std::io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    writeln!(w, "index,rtt_ns,status")?;
    for s in samples {
        let nanos = u64::try_from(s.rtt.as_nanos()).unwrap_or(u64::MAX);
        writeln!(w, "{},{nanos},{}", s.index, status_str(s.status))?;
    }
    w.flush()
}

fn status_str(s: SampleStatus) -> String {
    match s {
        SampleStatus::Ok => "ok".to_string(),
        SampleStatus::DeviceError(code) => format!("dev:{code:#04x}"),
        SampleStatus::Timeout => "timeout".to_string(),
        SampleStatus::LinkError => "link".to_string(),
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
