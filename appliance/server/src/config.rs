use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs_core::{CoreId, Interface, RtSchedulePolicy, ThreadPriority, ThreadPriorityValue};
use autd3_rs_link_echocat::EchocatLinkOption;
use autd3_rs_link_remote::{BusOption, BusPacing, BusServerOption};
use serde::Deserialize;

pub const RT_PRIORITY_OFF: u8 = 0;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub server: Server,
    pub bus: Bus,
    pub rt: Rt,
    pub health: Health,
    pub mdns: Mdns,
    pub control: Control,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
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
#[serde(deny_unknown_fields, default)]
pub struct Bus {
    pub interface: Option<String>,
    pub open_on_start: bool,
    #[serde(deserialize_with = "duration")]
    pub sync0_period: Option<Duration>,
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
            pdu_timeout: None,
            state_transition_timeout: None,
            process_data_watchdog: None,
            sync_timeout: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
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
            priority: DEFAULT_RT_PRIORITY,
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

#[cfg(not(target_os = "windows"))]
const DEFAULT_RT_PRIORITY: u8 = autd3_rs_core::rt::RT_THREAD_PRIORITY;
#[cfg(target_os = "windows")]
const DEFAULT_RT_PRIORITY: u8 = RT_PRIORITY_OFF;

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
#[serde(deny_unknown_fields, default)]
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
#[serde(deny_unknown_fields, default)]
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
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

fn duration<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<Duration>, D::Error> {
    let text = Option::<String>::deserialize(de)?;
    text.map(|text| humantime::parse_duration(&text).map_err(serde::de::Error::custom))
        .transpose()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read the config file {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("failed to parse the config file {}", path.display()))
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
        if self.rt.priority != RT_PRIORITY_OFF
            && ThreadPriorityValue::try_from(self.rt.priority).is_err()
        {
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
        for (name, value) in [
            ("bus.sync0_period", self.bus.sync0_period),
            ("bus.pdu_timeout", self.bus.pdu_timeout),
            (
                "bus.state_transition_timeout",
                self.bus.state_transition_timeout,
            ),
            ("bus.process_data_watchdog", self.bus.process_data_watchdog),
            ("bus.sync_timeout", self.bus.sync_timeout),
            ("health.report_interval", self.health.report_interval),
        ] {
            if value == Some(Duration::ZERO) {
                bail!("{name} must be greater than zero");
            }
        }
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

    fn rt_priority(&self) -> Option<ThreadPriority> {
        if self.rt.priority == RT_PRIORITY_OFF {
            return None;
        }
        ThreadPriorityValue::try_from(self.rt.priority)
            .ok()
            .map(ThreadPriority::Crossplatform)
    }
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
    fn an_unknown_key_is_an_error_rather_than_a_silent_default() {
        let err = toml::from_str::<Config>(
            r#"
            [bus]
            iface = "eth0"
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("iface"), "{err}");
    }
}
