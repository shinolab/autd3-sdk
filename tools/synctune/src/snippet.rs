use std::fmt::Write;
use std::time::Duration;

use crate::cli::{Common, LinkKind, Mode, RtPolicy};

pub fn print(common: &Common, sync0_period: Duration, sync0_shift: Duration) {
    println!("\n  reproduce this configuration in your app:");
    for line in render(common, sync0_period, sync0_shift).lines() {
        println!("    {line}");
    }
}

fn render(common: &Common, sync0_period: Duration, sync0_shift: Duration) -> String {
    let mut imports: Vec<&str> = vec![
        "std::time::Duration",
        "Client",
        "ClientConfig",
        "RtSchedulePolicy",
    ];
    let mut body = String::new();

    link_block(common, sync0_period, sync0_shift, &mut body);
    push_config(&mut body, &mut imports, &Config::from(common));

    let mut out = imports_block(common.link, &imports);
    out.push('\n');
    out.push_str(&body);
    out
}

fn link_block(common: &Common, sync0_period: Duration, sync0_shift: Duration, body: &mut String) {
    let (link, option) = match common.link {
        LinkKind::Soem => ("SoemLink", "SoemLinkOption"),
        LinkKind::Ethercrab => ("EtherCrabLink", "EtherCrabLinkOption"),
        LinkKind::Echocat => ("EchocatLink", "EchocatLinkOption"),
    };
    let open_arg = if common.link == LinkKind::Echocat {
        "&"
    } else {
        ""
    };
    let _ = writeln!(body, "let link = {link}::open({open_arg}{option} {{");
    if let Some(iface) = &common.interface {
        let _ = writeln!(body, "    iface: {iface:?}.into(),");
    }
    let _ = writeln!(body, "    sync0_period: {},", fmt_duration(sync0_period));
    if common.link != LinkKind::Echocat {
        let _ = writeln!(body, "    sync0_shift: {},", fmt_duration(sync0_shift));
    }
    let _ = writeln!(body, "    ..Default::default()");
    let _ = writeln!(body, "}})?;");
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
        LinkKind::Echocat => {
            let _ = writeln!(
                out,
                "use autd3_rs_link_echocat::{{EchocatLink, EchocatLinkOption}};",
            );
        }
        LinkKind::Soem => {
            let _ = writeln!(out, "use autd3_rs_link_soem::{{SoemLink, SoemLinkOption}};");
        }
    }
    out
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

impl From<&Common> for Config {
    fn from(c: &Common) -> Self {
        Config {
            timeout_cycles: c.timeout_cycles,
            max_inflight: match c.mode {
                Mode::StopAndWait => 1,
                Mode::Streaming => c.max_inflight.max(1),
            },
            max_resync_rounds: c.max_resync_rounds.get(),
            low_latency: c.low_latency,
            rt_priority: c.rt_priority,
            rt_policy: c.rt_policy,
            rt_affinity: c.rt_affinity,
        }
    }
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
