use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs_core::{CoreId, Interface, RtPriority, RtSchedulePolicy};
use autd3_rs_link_echocat::{EchocatLinkOption, FramePhase, MAX_SYNC0_PERIOD};
use autd3_rs_link_remote::{BusOption, BusPacing, BusServerOption};
use serde::Deserialize;

pub const RT_PRIORITY_OFF: u8 = 0;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub bus: Bus,
    pub rt: Rt,
    pub health: Health,
    pub mdns: Mdns,
    pub control: Control,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Server {
    pub bind: SocketAddr,
    pub auto_open: bool,
}

impl Default for Server {
    fn default() -> Self {
        let defaults = BusServerOption::default();
        Self {
            bind: defaults.bind,
            auto_open: defaults.auto_open,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Bus {
    pub interface: Option<String>,
    pub open_on_start: bool,
    #[serde(deserialize_with = "duration")]
    pub sync0_period: Option<Duration>,
    #[serde(deserialize_with = "frame_phase")]
    pub frame_phase: Option<FramePhase>,
    #[serde(deserialize_with = "duration")]
    pub pdu_timeout: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    pub state_transition_timeout: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    pub process_data_watchdog: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    pub sync_timeout: Option<Duration>,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            interface: None,
            open_on_start: true,
            sync0_period: None,
            frame_phase: None,
            pdu_timeout: None,
            state_transition_timeout: None,
            process_data_watchdog: None,
            sync_timeout: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Rt {
    pub priority: u8,
    pub policy: Policy,
    pub affinity: Option<usize>,
    pub lock_memory: bool,
    pub prefault_stack_bytes: usize,
}

impl Default for Rt {
    fn default() -> Self {
        Self {
            priority: default_rt_priority(),
            policy: Policy::Fifo,
            affinity: default_affinity(),
            lock_memory: true,
            prefault_stack_bytes: 512 * 1024,
        }
    }
}

fn available_cores() -> usize {
    online_cores().unwrap_or_else(|| {
        std::thread::available_parallelism().map_or(usize::MAX, std::num::NonZero::get)
    })
}

#[cfg(target_os = "linux")]
fn online_cores() -> Option<usize> {
    parse_cpu_list(&std::fs::read_to_string("/sys/devices/system/cpu/online").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn online_cores() -> Option<usize> {
    None
}

#[cfg(target_os = "linux")]
fn parse_cpu_list(list: &str) -> Option<usize> {
    list.trim()
        .split(',')
        .filter_map(|range| range.rsplit('-').next()?.trim().parse::<usize>().ok())
        .max()
        .map(|last| last + 1)
}

fn default_affinity() -> Option<usize> {
    let cores = available_cores();
    (cores != usize::MAX && cores > 1).then(|| cores - 1)
}

fn default_rt_priority() -> u8 {
    autd3_rs_core::default_rt_priority()
        .and_then(RtPriority::value)
        .unwrap_or(RT_PRIORITY_OFF)
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    Normal,
    #[default]
    Fifo,
    RoundRobin,
}

impl From<Policy> for RtSchedulePolicy {
    fn from(policy: Policy) -> Self {
        match policy {
            Policy::Normal => Self::Normal,
            Policy::Fifo => Self::Fifo,
            Policy::RoundRobin => Self::RoundRobin,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Health {
    #[serde(deserialize_with = "duration")]
    pub report_interval: Option<Duration>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            report_interval: Some(Duration::from_mins(1)),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Control {
    pub enabled: bool,
    pub bind: SocketAddr,
    pub allow_admin: bool,
    pub unit: String,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: SocketAddr::new(
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                autd3_rs_appliance::DEFAULT_CONTROL_PORT,
            ),
            allow_admin: true,
            unit: DEFAULT_UNIT.to_owned(),
        }
    }
}

const DEFAULT_UNIT: &str = "autd3-remote-server.service";

const MAX_TIMEOUT: Duration = Duration::from_hours(1);

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Mdns {
    pub enabled: bool,
    pub instance: Option<String>,
}

impl Default for Mdns {
    fn default() -> Self {
        Self {
            enabled: true,
            instance: None,
        }
    }
}

fn frame_phase<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<FramePhase>, D::Error> {
    let Some(text) = Option::<String>::deserialize(de)? else {
        return Ok(None);
    };
    if text.eq_ignore_ascii_case("auto") {
        return Ok(Some(FramePhase::Auto));
    }
    humantime::parse_duration(&text)
        .map(|at| Some(FramePhase::At(at)))
        .map_err(|e| serde::de::Error::custom(format!("{e}; use a duration or \"auto\"")))
}

fn duration<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<Duration>, D::Error> {
    let text = Option::<String>::deserialize(de)?;
    text.map(|text| humantime::parse_duration(&text).map_err(serde::de::Error::custom))
        .transpose()
}

impl Config {
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let mut unknown = Vec::new();
        let config = serde_ignored::deserialize(toml::Deserializer::parse(text)?, |path| {
            unknown.push(path.to_string());
        })?;
        Ok((config, unknown))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read the config file {}", path.display()))?;
        let (config, unknown) = Self::parse(&text)
            .with_context(|| format!("failed to parse the config file {}", path.display()))?;
        for key in &unknown {
            tracing::warn!(
                key,
                path = %path.display(),
                "the config has a key this server does not know; it was ignored",
            );
        }
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.bus.interface.is_none() {
            bail!(
                "the EtherCAT interface must be named explicitly. \
                 Probing every interface would also walk the uplink the client talks over, \
                 which is exactly what this appliance separates. \
                 Set `bus.interface` in the config file or pass --interface"
            );
        }
        if self.rt.priority != RT_PRIORITY_OFF && RtPriority::new(self.rt.priority).is_none() {
            bail!(
                "rt.priority must be 0 (off) or a valid scheduling priority, got {}",
                self.rt.priority
            );
        }
        if let Some(core) = self.rt.affinity {
            let cores = available_cores();
            if core >= cores {
                bail!(
                    "rt.affinity must be a CPU index below the core count ({cores}), got {core}. \
                     An out-of-range index leaves the bus thread unpinned, which costs SYNC0 \
                     deadlines under load"
                );
            }
        }
        for (name, value, max) in [
            ("bus.sync0_period", self.bus.sync0_period, MAX_SYNC0_PERIOD),
            ("bus.pdu_timeout", self.bus.pdu_timeout, MAX_TIMEOUT),
            (
                "bus.state_transition_timeout",
                self.bus.state_transition_timeout,
                MAX_TIMEOUT,
            ),
            (
                "bus.process_data_watchdog",
                self.bus.process_data_watchdog,
                MAX_TIMEOUT,
            ),
            ("bus.sync_timeout", self.bus.sync_timeout, MAX_TIMEOUT),
            (
                "health.report_interval",
                self.health.report_interval,
                MAX_TIMEOUT,
            ),
        ] {
            let Some(value) = value else { continue };
            if value.is_zero() {
                bail!("{name} must be greater than zero");
            }
            if value > max {
                bail!("{name} must be at most {max:?}, got {value:?}");
            }
        }
        if let Some(FramePhase::At(at)) = self.bus.frame_phase {
            let period = self
                .bus
                .sync0_period
                .unwrap_or_else(|| EchocatLinkOption::default().sync0_period);
            if at.is_zero() || at >= period {
                bail!(
                    "bus.frame_phase must land inside the SYNC0 period (0 < phase < {period:?}), \
                     got {at:?}. A frame landing on the SYNC0 edge is dropped as a sequence \
                     mismatch"
                );
            }
        }
        self.link_option()
            .validate()
            .map_err(|e| anyhow::anyhow!("[bus] {e}"))?;
        Ok(())
    }

    pub fn link_option(&self) -> EchocatLinkOption {
        let defaults = EchocatLinkOption::default();
        EchocatLinkOption {
            iface: self
                .bus
                .interface
                .clone()
                .map_or(Interface::Auto, Interface::Name),
            sync0_period: self.bus.sync0_period.unwrap_or(defaults.sync0_period),
            frame_phase: self.bus.frame_phase.unwrap_or(defaults.frame_phase),
            pdu_timeout: self.bus.pdu_timeout.unwrap_or(defaults.pdu_timeout),
            state_transition_timeout: self
                .bus
                .state_transition_timeout
                .unwrap_or(defaults.state_transition_timeout),
            process_data_watchdog: self
                .bus
                .process_data_watchdog
                .unwrap_or(defaults.process_data_watchdog),
            sync_timeout: self.bus.sync_timeout.unwrap_or(defaults.sync_timeout),
            ..defaults
        }
    }

    pub fn server_option(&self) -> BusServerOption {
        BusServerOption {
            bind: self.server.bind,
            auto_open: self.server.auto_open,
            ..BusServerOption::default()
        }
    }

    pub fn bus_option(&self) -> BusOption {
        BusOption {
            pacing: BusPacing::LinkPaced,
            rt_priority: self.rt_priority(),
            rt_policy: self.rt.policy.into(),
            rt_affinity: self.rt.affinity.map(|id| CoreId { id }),
            stack_prefault_bytes: self.rt.prefault_stack_bytes,
        }
    }

    fn rt_priority(&self) -> Option<RtPriority> {
        if self.rt.priority == RT_PRIORITY_OFF {
            return None;
        }
        RtPriority::new(self.rt.priority)
    }
}

const BUS_SECTION: &str = "bus";

pub fn set_bus_timing(text: &str, sync0_period: &str, frame_phase: &str) -> String {
    set_keys(
        text,
        BUS_SECTION,
        &[("sync0_period", sync0_period), ("frame_phase", frame_phase)],
    )
}

fn set_keys(text: &str, section: &str, entries: &[(&str, &str)]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut written = vec![false; entries.len()];
    let mut in_section = false;
    let mut section_found = false;
    let mut insert_at = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = header_name(trimmed) {
            in_section = name == section;
            out.push(line.to_owned());
            if in_section {
                section_found = true;
                insert_at = Some(out.len());
            }
            continue;
        }
        let entry = (in_section && !trimmed.starts_with('#'))
            .then(|| key_of(trimmed))
            .flatten()
            .and_then(|key| entries.iter().position(|(name, _)| *name == key));
        match entry {
            Some(index) => {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push(format!(
                    "{indent}{} = \"{}\"",
                    entries[index].0, entries[index].1,
                ));
                written[index] = true;
            }
            None => out.push(line.to_owned()),
        }
        if in_section && !trimmed.is_empty() {
            insert_at = Some(out.len());
        }
    }

    let missing: Vec<String> = entries
        .iter()
        .zip(&written)
        .filter(|(_, written)| !**written)
        .map(|((key, value), _)| format!("{key} = \"{value}\""))
        .collect();
    if !missing.is_empty() {
        if let Some(at) = insert_at.filter(|_| section_found) {
            out.splice(at..at, missing);
        } else {
            if out.last().is_some_and(|line| !line.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{section}]"));
            out.extend(missing);
        }
    }

    let mut result = out.join("\n");
    if !result.is_empty() && (text.is_empty() || text.ends_with('\n')) {
        result.push('\n');
    }
    result
}

fn header_name(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    (!inner.starts_with('[')).then(|| inner.trim().trim_matches('"'))
}

fn key_of(trimmed: &str) -> Option<&str> {
    let key = trimmed.split_once('=')?.0.trim();
    (!key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_config_still_needs_an_interface() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_config_file_drives_every_section() {
        let core = available_cores() - 1;
        let config: Config = toml::from_str(&format!(
            r#"
            [server]
            bind = "10.0.0.2:9000"
            auto_open = false

            [bus]
            interface = "eth0"
            open_on_start = false
            sync0_period = "2ms"
            pdu_timeout = "50ms"

            [rt]
            priority = 60
            policy = "round-robin"
            affinity = {core}
            lock_memory = false

            [health]
            report_interval = "5s"
            "#,
        ))
        .unwrap();
        config.validate().unwrap();

        let link = config.link_option();
        assert_eq!(link.iface.name(), Some("eth0"));
        assert_eq!(link.sync0_period, Duration::from_millis(2));
        assert_eq!(link.pdu_timeout, Duration::from_millis(50));
        assert_eq!(
            link.state_transition_timeout,
            EchocatLinkOption::default().state_transition_timeout,
        );

        let server = config.server_option();
        assert_eq!(server.bind, "10.0.0.2:9000".parse().unwrap());
        assert!(!server.auto_open);

        let bus = config.bus_option();
        assert_eq!(bus.pacing, BusPacing::LinkPaced);
        assert_eq!(bus.rt_policy, RtSchedulePolicy::RoundRobin);
        assert_eq!(bus.rt_affinity.map(|c| c.id), Some(core));
        assert!(bus.rt_priority.is_some());
        assert_eq!(bus.stack_prefault_bytes, 512 * 1024);

        assert!(!config.rt.lock_memory);
        assert!(!config.bus.open_on_start);
        assert_eq!(config.health.report_interval, Some(Duration::from_secs(5)));
    }

    #[test]
    fn the_bus_thread_is_pinned_unless_the_config_says_otherwise() {
        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            "#,
        )
        .unwrap();
        assert_eq!(
            config.bus_option().rt_affinity.map(|c| c.id),
            default_affinity(),
            "leaving the bus thread unpinned costs gaps under load",
        );
    }

    #[test]
    fn a_zero_priority_turns_the_real_time_scheduling_off() {
        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            [rt]
            priority = 0
            "#,
        )
        .unwrap();
        config.validate().unwrap();
        assert!(config.bus_option().rt_priority.is_none());
    }

    #[test]
    fn an_affinity_past_the_last_core_is_rejected_rather_than_silently_unpinned() {
        let config: Config = toml::from_str(&format!(
            r#"
            [bus]
            interface = "eth0"
            [rt]
            affinity = {}
            "#,
            available_cores(),
        ))
        .unwrap();
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("rt.affinity"), "{err}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_core_count_comes_from_the_online_list_not_the_affinity_mask() {
        assert_eq!(parse_cpu_list("0-3\n"), Some(4));
        assert_eq!(parse_cpu_list("0"), Some(1));
        assert_eq!(parse_cpu_list("0-1,3"), Some(4));
        assert_eq!(parse_cpu_list(""), None);
        assert!(
            available_cores()
                >= std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
        );
    }

    #[test]
    fn an_absurdly_long_duration_is_rejected() {
        for (key, value) in [
            ("sync0_period", "5s"),
            ("pdu_timeout", "2h"),
            ("state_transition_timeout", "2h"),
            ("process_data_watchdog", "2h"),
            ("sync_timeout", "2h"),
        ] {
            let config: Config = toml::from_str(&format!(
                r#"
                [bus]
                interface = "eth0"
                {key} = "{value}"
                "#,
            ))
            .unwrap();
            let err = config.validate().unwrap_err().to_string();
            assert!(err.contains(key), "{err}");
        }
    }

    #[test]
    fn a_config_that_validates_yields_a_link_option_the_link_accepts() {
        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            sync0_period = "2ms"
            pdu_timeout = "50ms"
            "#,
        )
        .unwrap();
        config.validate().unwrap();
        assert!(config.link_option().validate().is_ok());
    }

    #[test]
    fn a_zero_duration_is_rejected() {
        for key in [
            "sync0_period",
            "pdu_timeout",
            "state_transition_timeout",
            "process_data_watchdog",
            "sync_timeout",
        ] {
            let config: Config = toml::from_str(&format!(
                r#"
                [bus]
                interface = "eth0"
                {key} = "0s"
                "#,
            ))
            .unwrap();
            let err = config.validate().unwrap_err().to_string();
            assert!(err.contains(key), "{err}");
        }

        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            [health]
            report_interval = "0s"
            "#,
        )
        .unwrap();
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("health.report_interval"), "{err}");
    }

    #[test]
    fn the_landing_phase_defaults_to_following_the_exchange() {
        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            "#,
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.link_option().frame_phase, FramePhase::Auto);
    }

    #[test]
    fn the_landing_phase_can_be_pinned_or_left_to_the_exchange() {
        for (text, expected) in [
            ("\"500us\"", FramePhase::At(Duration::from_micros(500))),
            ("\"auto\"", FramePhase::Auto),
            ("\"AUTO\"", FramePhase::Auto),
        ] {
            let config: Config = toml::from_str(&format!(
                r#"
                [bus]
                interface = "eth0"
                sync0_period = "2ms"
                frame_phase = {text}
                "#,
            ))
            .unwrap();
            config.validate().unwrap();
            assert_eq!(config.link_option().frame_phase, expected);
        }
    }

    #[test]
    fn a_landing_phase_outside_the_sync0_period_is_rejected() {
        for text in ["\"0s\"", "\"2ms\"", "\"5ms\""] {
            let config: Config = toml::from_str(&format!(
                r#"
                [bus]
                interface = "eth0"
                sync0_period = "2ms"
                frame_phase = {text}
                "#,
            ))
            .unwrap();
            let err = config.validate().unwrap_err().to_string();
            assert!(err.contains("bus.frame_phase"), "{text}: {err}");
        }
    }

    #[test]
    fn a_landing_phase_that_is_neither_a_duration_nor_auto_is_an_error() {
        let err = toml::from_str::<Config>(
            r#"
            [bus]
            interface = "eth0"
            frame_phase = "middle"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("auto"), "{err}");
    }

    #[test]
    fn an_unknown_key_is_reported_but_does_not_stop_the_load() {
        let (config, unknown) = Config::parse(
            r#"
            [bus]
            interface = "eth0"
            iface = "eth1"

            [future]
            knob = 1
            "#,
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.bus.interface.as_deref(), Some("eth0"));
        assert_eq!(unknown, vec!["bus.iface".to_owned(), "future".to_owned()]);
    }

    #[test]
    fn a_config_this_server_fully_understands_reports_nothing() {
        let (_, unknown) = Config::parse(include_str!("../dist/remote-server.toml")).unwrap();
        assert_eq!(unknown, Vec::<String>::new());
    }

    #[test]
    fn a_malformed_config_is_still_an_error() {
        assert!(Config::parse("[bus").is_err());
    }

    #[test]
    fn applying_a_tuned_candidate_touches_only_the_two_keys_it_owns() {
        let before = include_str!("../dist/remote-server.toml");
        let after = set_bus_timing(before, "2ms", "auto");
        assert!(after.contains("sync0_period = \"2ms\""), "{after}");
        assert!(after.contains("frame_phase = \"auto\""), "{after}");
        assert!(after.contains("# instance = \"autd3-lab-1\""), "{after}");
        assert_eq!(
            before.lines().count(),
            after.lines().count(),
            "no line may be added or dropped when both keys already exist",
        );
        for (old, new) in before.lines().zip(after.lines()) {
            let key = old.split_once('=').map(|(key, _)| key.trim());
            if key == Some("sync0_period") || key == Some("frame_phase") {
                continue;
            }
            assert_eq!(old, new, "only the tuned keys may move");
        }
        let (mut config, unknown) = Config::parse(&after).unwrap();
        // 配布 config は 4 コアのボード向けに `affinity = 3` を持つ. CI のランナーは
        // それより小さいことがあるので, ここで見たいのは affinity ではないと明示する.
        config.rt.affinity = default_affinity();
        config.validate().unwrap();
        assert_eq!(unknown, Vec::<String>::new());
        assert_eq!(config.bus.sync0_period, Some(Duration::from_millis(2)));
        assert_eq!(config.link_option().frame_phase, FramePhase::Auto);
    }

    #[test]
    fn applying_a_tuned_candidate_adds_the_keys_the_config_does_not_carry_yet() {
        let after = set_bus_timing(
            "[bus]\ninterface = \"eth0\"\n\n[mdns]\nenabled = true\n",
            "500us",
            "125us",
        );
        assert_eq!(
            after,
            "[bus]\ninterface = \"eth0\"\nsync0_period = \"500us\"\nframe_phase = \"125us\"\n\n\
             [mdns]\nenabled = true\n",
        );
        let (config, _) = Config::parse(&after).unwrap();
        config.validate().unwrap();
        assert_eq!(
            config.link_option().frame_phase,
            FramePhase::At(Duration::from_micros(125)),
        );
    }

    #[test]
    fn applying_a_tuned_candidate_writes_a_bus_section_when_there_is_none() {
        assert_eq!(
            set_bus_timing("[mdns]\nenabled = true\n", "1ms", "auto"),
            "[mdns]\nenabled = true\n\n[bus]\nsync0_period = \"1ms\"\nframe_phase = \"auto\"\n",
        );
        assert_eq!(
            set_bus_timing("", "1ms", "auto"),
            "[bus]\nsync0_period = \"1ms\"\nframe_phase = \"auto\"\n",
        );
    }

    #[test]
    fn applying_a_tuned_candidate_leaves_commented_out_keys_alone() {
        let after = set_bus_timing(
            "[bus]\n  # sync0_period = \"1ms\"\n  sync0_period = \"1ms\"\n",
            "3ms",
            "auto",
        );
        assert_eq!(
            after,
            "[bus]\n  # sync0_period = \"1ms\"\n  sync0_period = \"3ms\"\n\
             frame_phase = \"auto\"\n",
            "the comment records what the user wrote; only the live key is the setting",
        );
    }

    #[test]
    fn a_key_in_another_section_is_never_mistaken_for_the_bus_one() {
        let after = set_bus_timing(
            "[health]\nsync0_period = \"9ms\"\n\n[bus]\nsync0_period = \"1ms\"\nframe_phase = \"auto\"\n",
            "2ms",
            "25%",
        );
        assert!(
            after.contains("[health]\nsync0_period = \"9ms\""),
            "{after}"
        );
        assert!(after.contains("[bus]\nsync0_period = \"2ms\""), "{after}");
    }

    #[test]
    fn the_shipped_config_is_still_a_valid_one() {
        let mut config: Config =
            toml::from_str(include_str!("../dist/remote-server.toml")).unwrap();
        config.rt.affinity = default_affinity();
        config.validate().unwrap();
        assert_eq!(config.link_option().frame_phase, FramePhase::Auto);
        assert_eq!(config.bus.interface.as_deref(), Some("eth0"));
    }
}
