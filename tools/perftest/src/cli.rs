use std::net::IpAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_IN_FLIGHT;
use autd3_rs::operation::XOR_HASH_MAX_DATA_LEN;
use autd3_rs_link_twincat::{AmsNetId, TwinCATRoute};
use clap::{ArgGroup, Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    StopAndWait,
    Streaming,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Ethercrab,
    Soem,
    Twincat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum TwincatRoute {
    #[default]
    Auto,
    Notify,
    Ads,
}

impl From<TwincatRoute> for TwinCATRoute {
    fn from(route: TwincatRoute) -> Self {
        match route {
            TwincatRoute::Auto => TwinCATRoute::Auto,
            TwincatRoute::Notify => TwinCATRoute::Notify,
            TwincatRoute::Ads => TwinCATRoute::Ads,
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "autd3-rs-perftest",
    about,
    group(ArgGroup::new("stop").args(["count", "duration"]).required(true).multiple(false))
)]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = LinkKind::Ethercrab)]
    pub link: LinkKind,
    #[arg(long, default_value = None)]
    pub interface: Option<String>,
    #[arg(long)]
    pub devices: Option<usize>,
    #[arg(long, default_value_t = XOR_HASH_MAX_DATA_LEN)]
    pub data_len: usize,
    #[arg(long, default_value_t = 0)]
    pub sleep_ms: u16,
    #[arg(long, default_value_t = 1000)]
    pub cycle_us: u64,
    #[arg(long, default_value_t = 0)]
    pub shift_percent: u8,
    #[arg(long)]
    pub count: Option<u64>,
    #[arg(long, value_parser = humantime::parse_duration)]
    pub duration: Option<Duration>,
    #[arg(long, default_value_t = 0)]
    pub warmup: u64,
    #[arg(long)]
    pub csv: Option<PathBuf>,
    #[arg(long, default_value_t = 10)]
    pub timeout_cycles: u32,
    #[arg(long, value_enum, default_value_t = Mode::StopAndWait)]
    pub mode: Mode,
    #[arg(long, default_value_t = MAX_IN_FLIGHT)]
    pub inflight: usize,
    #[arg(long, default_value_t = NonZeroU32::new(1).unwrap())]
    pub send_interval_cycles: NonZeroU32,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap())]
    pub max_resync_rounds: NonZeroU32,
    #[arg(long, default_value_t = false)]
    pub low_latency: bool,
    #[arg(long)]
    pub twincat_remote: Option<IpAddr>,
    #[arg(long)]
    pub ams_net_id: Option<AmsNetId>,
    #[arg(long, value_enum, default_value_t = TwincatRoute::default())]
    pub twincat_route: TwincatRoute,
    #[arg(long, default_value_t = false)]
    pub no_win_perf_tune: bool,
    #[arg(long)]
    pub rt_priority: Option<u8>,
    #[arg(long)]
    pub rt_core: Option<usize>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.data_len > XOR_HASH_MAX_DATA_LEN {
            return Err(format!(
                "--data-len {} exceeds XOR_HASH_MAX_DATA_LEN ({XOR_HASH_MAX_DATA_LEN})",
                self.data_len,
            ));
        }
        if self.mode == Mode::Streaming && (self.inflight == 0 || self.inflight > MAX_IN_FLIGHT) {
            return Err(format!(
                "--inflight {} must be in 1..={MAX_IN_FLIGHT}",
                self.inflight,
            ));
        }
        if self.shift_percent > 100 {
            return Err(format!(
                "--shift-percent {} must be in 0..=100",
                self.shift_percent
            ));
        }
        if self.link == LinkKind::Twincat {
            if self.twincat_remote.is_some() && self.ams_net_id.is_none() {
                return Err("--ams-net-id is required when --twincat-remote is set".to_string());
            }
        } else if self.twincat_remote.is_some() || self.ams_net_id.is_some() {
            return Err(
                "--twincat-remote / --ams-net-id are only valid with --link twincat".to_string(),
            );
        }
        if let Some(p) = self.rt_priority
            && p > 99
        {
            return Err(format!("--rt-priority {p} must be in 0..=99"));
        }
        Ok(())
    }
}
