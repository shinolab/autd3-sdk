use std::fmt::Write;
use std::time::Duration;

use crate::cli::{Cli, LinkKind, Mode, RtPolicy};

pub fn print(cli: &Cli) {
    println!();
    println!("=== reproduce this configuration in your app ===");
    print!("{}", render(cli));
}

fn render(cli: &Cli) -> String {
    let mut imports: Vec<&str> = vec!["Client", "ClientConfig", "RtSchedulePolicy"];
    let mut body = String::new();

    link_block(cli, &mut body, &mut imports);
    push_config(&mut body, &mut imports, &Config::from(cli));

    let mut out = imports_block(cli.link, &imports);
    out.push('\n');
    out.push_str(&body);
    out
}

fn link_block(cli: &Cli, body: &mut String, imports: &mut Vec<&'static str>) {
    let sync0_period = cli.sync0_period;
    let sync0_shift = shift_duration(cli.sync0_period, cli.shift_percent);
    match cli.link {
        LinkKind::Ethercrab | LinkKind::Soem => {
            let (link, option) = match cli.link {
                LinkKind::Soem => ("SoemLink", "SoemLinkOption"),
                _ => ("EtherCrabLink", "EtherCrabLinkOption"),
            };
            imports.push("std::time::Duration");
            let _ = writeln!(body, "let link = {link}::open({option} {{");
            if let Some(iface) = &cli.interface {
                let _ = writeln!(body, "    iface: {iface:?}.into(),");
            }
            let _ = writeln!(body, "    sync0_period: {},", fmt_duration(sync0_period));
            let _ = writeln!(body, "    sync0_shift: {},", fmt_duration(sync0_shift));
            let _ = writeln!(body, "    ..Default::default()");
            let _ = writeln!(body, "}})?;");
        }
        LinkKind::Twincat => {
            let _ = writeln!(
                body,
                "let link = TwinCATLink::open(TwinCATLinkOption::local())?; \
                 // or ::remote(addr, ams_net_id)",
            );
        }
        LinkKind::Nop => {
            let _ = writeln!(
                body,
                "// --link nop is a benchmark stub; use a real link (ethercrab/soem/twincat) here.",
            );
        }
    }
}

impl From<&Cli> for Config {
    fn from(cli: &Cli) -> Self {
        Config {
            timeout_cycles: cli.timeout_cycles,
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

fn imports_block(link: LinkKind, imports: &[&str]) -> String {
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
    match link {
        LinkKind::Ethercrab => {
            let _ = writeln!(
                out,
                "use autd3_rs_link_ethercrab::{{EtherCrabLink, EtherCrabLinkOption}};",
            );
        }
        LinkKind::Soem => {
            let _ = writeln!(out, "use autd3_rs_link_soem::{{SoemLink, SoemLinkOption}};");
        }
        LinkKind::Twincat => {
            let _ = writeln!(
                out,
                "use autd3_rs_link_twincat::{{TwinCATLink, TwinCATLinkOption}};",
            );
        }
        LinkKind::Nop => {}
    }
    out
}

fn shift_duration(period: Duration, percent: u8) -> Duration {
    let nanos = period.as_nanos() * u128::from(percent) / 100;
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
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
    timeout_cycles: u32,
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
    let _ = writeln!(body, "    timeout_cycles: {},", c.timeout_cycles);
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
        need("ThreadPriority");
        need("ThreadPriorityValue");
        let _ = writeln!(
            body,
            "    rt_priority: Some(ThreadPriority::Crossplatform(\
             ThreadPriorityValue::try_from({p}).unwrap())),",
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
        "let client = Client::open(&geometry, link, config).await?;"
    );
}
