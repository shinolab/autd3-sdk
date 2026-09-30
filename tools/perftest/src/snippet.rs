use std::fmt::Write;
use std::time::Duration;

use crate::cli::{Cli, Mode, RtPolicy};

pub fn print(cli: &Cli) {
    println!();
    println!("=== reproduce this configuration in your app ===");
    print!("{}", render(cli));
}

fn render(cli: &Cli) -> String {
    let mut imports: Vec<&str> = vec!["Client", "ClientConfig", "Driver", "TransportOption"];
    let mut body = String::new();

    option_block(cli, &mut body, &mut imports);
    push_config(&mut body, &mut imports, &Config::from(cli));
    push_open(&mut body, &Config::from(cli));

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
        .filter(|s| !s.starts_with("std::") && !s.starts_with("thread_priority::"))
        .collect();
    let _ = writeln!(out, "use autd3_rs::{{{}}};", autd.join(", "));
    for imp in imports
        .iter()
        .filter(|s| s.starts_with("thread_priority::"))
    {
        let _ = writeln!(out, "use {imp};");
    }
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
        RtPolicy::Normal => "ThreadSchedulePolicy::Normal(NormalThreadSchedulePolicy::Other)",
        RtPolicy::Fifo => "ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo)",
        RtPolicy::RoundRobin => {
            "ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::RoundRobin)"
        }
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
    let _ = writeln!(body, "    ..Default::default()");
    let _ = writeln!(body, "}};");
    if c.rt_priority.is_some() {
        need("thread_priority::*");
    }
}

fn push_open(body: &mut String, c: &Config) {
    let _ = writeln!(
        body,
        "let (mut driver, connector) = Driver::open(&option, geometry.num_devices())?;"
    );
    if c.rt_priority.is_none() && c.rt_affinity.is_none() {
        let _ = writeln!(body, "std::thread::spawn(move || driver.run());");
    } else {
        let _ = writeln!(body, "std::thread::spawn(move || {{");
        if let Some(p) = c.rt_priority {
            let _ = writeln!(
                body,
                "    let priority = ThreadPriority::Crossplatform({p}u8.try_into().unwrap());"
            );
            if cfg!(target_os = "linux") {
                let _ = writeln!(
                    body,
                    "    let _ = set_thread_priority_and_policy(thread_native_id(), priority, {});",
                    rt_policy(c.rt_policy),
                );
            } else {
                let _ = writeln!(body, "    let _ = set_current_thread_priority(priority);");
            }
        }
        if let Some(id) = c.rt_affinity {
            let _ = writeln!(
                body,
                "    core_affinity::set_for_current(core_affinity::CoreId {{ id: {id} }});"
            );
        }
        let _ = writeln!(body, "    driver.run()");
        let _ = writeln!(body, "}});");
    }
    let _ = writeln!(
        body,
        "let client = Client::open(&geometry, connector, config).await?;"
    );
}
