use std::fmt::Write;
use std::time::Duration;

use crate::cli::{Cli, Mode, RtPolicy};

pub fn print(cli: &Cli) {
    println!();
    println!("=== reproduce this configuration in your app ===");
    print!("{}", render(cli));
}

fn render(cli: &Cli) -> String {
    let mut imports: Vec<&str> = vec![
        "Client",
        "ClientConfig",
        "RtSchedulePolicy",
        "TransportOption",
    ];
    let mut body = String::new();

    option_block(cli, &mut body, &mut imports);
    push_config(&mut body, &mut imports, &Config::from(cli));

    let mut out = imports_block(&imports);
    out.push('\n');
    out.push_str(&body);
    out
}

fn option_block(cli: &Cli, body: &mut String, imports: &mut Vec<&'static str>) {
    imports.push("std::time::Duration");
    let _ = writeln!(body, "let option = TransportOption {{");
    if let Some(iface) = &cli.interface {
        let _ = writeln!(body, "    iface: {iface:?}.into(),");
    }
    if let Some(group) = &cli.group {
        let _ = writeln!(body, "    group: Some(\"{group}\".parse()?),");
    }
    let _ = writeln!(body, "    heartbeat: {},", fmt_duration(cli.heartbeat));
    if let Some(reply_timeout) = cli.reply_timeout {
        let _ = writeln!(body, "    reply_timeout: {},", fmt_duration(reply_timeout));
    }
    let _ = writeln!(body, "    ..Default::default()");
    let _ = writeln!(body, "}};");
}

impl From<&Cli> for Config {
    fn from(cli: &Cli) -> Self {
        Config {
            ack_timeout: cli.ack_timeout,
            max_inflight: match cli.mode {
                Mode::StopAndWait => 1,
                Mode::Streaming => cli.max_inflight.max(1),
            },
            max_resync_rounds: cli.max_resync_rounds.get(),
            low_latency: cli.low_latency,
            rt_priority: cli.rt_priority,
            rt_policy: cli.rt_policy,
            rt_affinity: cli.rt_affinity,
        }
    }
}

fn imports_block(imports: &[&str]) -> String {
    let mut out = String::new();
    for imp in imports.iter().filter(|s| s.starts_with("std::")) {
        let _ = writeln!(out, "use {imp};");
    }
    let autd: Vec<&str> = imports
        .iter()
        .copied()
        .filter(|s| !s.starts_with("std::"))
        .collect();
    let _ = writeln!(out, "use autd3_rs::{{{}}};", autd.join(", "));
    out
}

fn fmt_duration(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns == 0 {
        "Duration::ZERO".to_string()
    } else if ns.is_multiple_of(1_000_000) {
        format!("Duration::from_millis({})", ns / 1_000_000)
    } else if ns.is_multiple_of(1_000) {
        format!("Duration::from_micros({})", ns / 1_000)
    } else {
        format!("Duration::from_nanos({ns})")
    }
}

fn rt_policy(p: RtPolicy) -> &'static str {
    match p {
        RtPolicy::Normal => "RtSchedulePolicy::Normal",
        RtPolicy::Fifo => "RtSchedulePolicy::Fifo",
        RtPolicy::RoundRobin => "RtSchedulePolicy::RoundRobin",
    }
}

struct Config {
    ack_timeout: Duration,
    max_inflight: usize,
    max_resync_rounds: u32,
    low_latency: bool,
    rt_priority: Option<u8>,
    rt_policy: RtPolicy,
    rt_affinity: Option<usize>,
}

fn push_config(body: &mut String, imports: &mut Vec<&'static str>, c: &Config) {
    let mut need = |sym: &'static str| {
        if !imports.contains(&sym) {
            imports.push(sym);
        }
    };
    need("std::num::NonZeroUsize");
    need("std::num::NonZeroU32");
    let _ = writeln!(body, "let config = ClientConfig {{");
    let _ = writeln!(body, "    ack_timeout: {},", fmt_duration(c.ack_timeout));
    let _ = writeln!(
        body,
        "    max_inflight: NonZeroUsize::new({}).unwrap(),",
        c.max_inflight,
    );
    let _ = writeln!(
        body,
        "    max_resync_rounds: NonZeroU32::new({}).unwrap(),",
        c.max_resync_rounds,
    );
    if c.low_latency {
        let _ = writeln!(body, "    low_latency: true,");
    }
    if let Some(p) = c.rt_priority {
        need("RtPriority");
        let _ = writeln!(
            body,
            "    rt_priority: Some(RtPriority::new({p}).unwrap()),",
        );
    }
    let _ = writeln!(body, "    rt_policy: {},", rt_policy(c.rt_policy));
    if let Some(id) = c.rt_affinity {
        need("CoreId");
        let _ = writeln!(body, "    rt_affinity: Some(CoreId {{ id: {id} }}),");
    }
    let _ = writeln!(body, "    ..Default::default()");
    let _ = writeln!(body, "}};");
    let _ = writeln!(
        body,
        "let client = Client::open(&geometry, option, config).await?;"
    );
}
