use std::net::IpAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_INFLIGHT;
use autd3_rs_link_echocat::SleepStrategy;
use autd3_rs_link_twincat::AmsNetId;
use clap::{ArgGroup, Parser, ValueEnum};

pub const DEFAULT_MAX_SAMPLES: u64 = 1_000_000;

#[cfg(target_os = "windows")]
pub const DEFAULT_SYNC0_PERIOD: &str = "2ms";
#[cfg(not(target_os = "windows"))]
pub const DEFAULT_SYNC0_PERIOD: &str = "1ms";

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
    Echocat,
    Soem,
    Twincat,
    Nop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Command {
    Nop,
    WritePatternBuffer,
    #[default]
    Pattern,
}

impl Command {
    pub const fn is_pattern(self) -> bool {
        matches!(self, Self::WritePatternBuffer | Self::Pattern)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum SleepStrategyArg {
    #[default]
    Sleep,
    Spin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum RtPolicy {
    Normal,
    #[default]
    Fifo,
    RoundRobin,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "autd3-rs-perftest",
    about,
    group(ArgGroup::new("stop").args(["count", "duration"]).multiple(false))
)]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = LinkKind::Ethercrab)]
    pub link: LinkKind,
    #[arg(
        long,
        value_enum,
        default_value_t = Command::Pattern,
        help = "Command to measure. nop touches no FPGA register (pure link path), \
                write-pattern-buffer writes FPGA RAM without latching, \
                pattern is the fused write+config+bank-change that latches CTL_FLAG once per frame."
    )]
    pub command: Command,
    #[arg(
        long,
        default_value = None,
        help = "EtherCAT network interface"
    )]
    pub interface: Option<String>,
    #[arg(long)]
    pub devices: Option<usize>,
    #[arg(
        long = "sync0-period",
        value_parser = humantime::parse_duration,
        default_value = DEFAULT_SYNC0_PERIOD,
        help = "SYNC0 / EtherCAT cycle period, e.g. 1ms / 500us (maps to *LinkOption.sync0_period). \
                Defaults to 2ms on Windows (absorbs DPC wake jitter, matches the ethercrab/soem Windows preset; echocat keeps 1ms everywhere) and 1ms elsewhere."
    )]
    pub sync0_period: Duration,
    #[arg(
        long,
        default_value_t = 0,
        help = "SYNC0 shift as a percent of the period (maps to *LinkOption.sync0_shift = period * percent)."
    )]
    pub shift_percent: u8,
    #[arg(
        long = "sleep-strategy",
        value_enum,
        default_value_t = SleepStrategyArg::Sleep,
        help = "--link echocat only: how the RT thread waits for the next cycle (maps to EchocatLinkOption.sleep_strategy)."
    )]
    pub sleep_strategy: SleepStrategyArg,
    #[arg(
        long = "spin-margin",
        value_parser = humantime::parse_duration,
        default_value = "1ms",
        help = "How long before the deadline --sleep-strategy spin stops sleeping and busy-waits. \
                Must exceed how far the OS oversleeps (0.5-0.7ms on Windows even under timeBeginPeriod(1))."
    )]
    pub spin_margin: Duration,
    #[arg(long)]
    pub count: Option<u64>,
    #[arg(long, value_parser = humantime::parse_duration)]
    pub duration: Option<Duration>,
    #[arg(long, default_value_t = 0)]
    pub warmup: u64,
    #[arg(
        long,
        default_value_t = DEFAULT_MAX_SAMPLES,
        help = "Cap on retained per-send samples (0 = unlimited). Sends continue past the cap \
                but are no longer recorded, so an unbounded run has bounded memory."
    )]
    pub max_samples: u64,
    #[arg(
        long,
        default_value_t = false,
        help = "Emit BaseSignal on GPIO[0] to probe inter-device sync on a scope"
    )]
    pub gpio_base_signal: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Stop at the first failed send and exit non-zero (soak testing). \
                The summary is still printed."
    )]
    pub stop_on_error: bool,
    #[arg(long)]
    pub csv: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = 10,
        help = "maps to ClientConfig.timeout_cycles"
    )]
    pub timeout_cycles: u32,
    #[arg(long, value_enum, default_value_t = Mode::StopAndWait)]
    pub mode: Mode,
    #[arg(
        long = "max-inflight",
        alias = "inflight",
        default_value_t = MAX_INFLIGHT,
        help = "Pipeline depth in streaming mode (maps to ClientConfig.max_inflight). Ignored in stop-and-wait."
    )]
    pub max_inflight: usize,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap(), help = "maps to ClientConfig.max_resync_rounds")]
    pub max_resync_rounds: NonZeroU32,
    #[arg(
        long,
        default_value_t = false,
        help = "maps to ClientConfig.low_latency"
    )]
    pub low_latency: bool,
    #[arg(long)]
    pub twincat_remote: Option<IpAddr>,
    #[arg(long)]
    pub ams_net_id: Option<AmsNetId>,
    #[arg(long, default_value_t = false)]
    pub no_win_perf_tune: bool,
    #[arg(
        long,
        help = "maps to ClientConfig.rt_priority (0..=99). Omit to keep the library default \
                (TimeCritical on Windows, unset elsewhere)."
    )]
    pub rt_priority: Option<u8>,
    #[arg(long, value_enum, default_value_t = RtPolicy::Fifo, help = "maps to ClientConfig.rt_policy")]
    pub rt_policy: RtPolicy,
    #[arg(
        long = "rt-affinity",
        alias = "rt-core",
        help = "Pin the RT thread to this CPU core (maps to ClientConfig.rt_affinity)."
    )]
    pub rt_affinity: Option<usize>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Streaming
            && (self.max_inflight == 0 || self.max_inflight > MAX_INFLIGHT)
        {
            return Err(format!(
                "--max-inflight {} must be in 1..={MAX_INFLIGHT}",
                self.max_inflight,
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
        if self.link == LinkKind::Echocat && self.shift_percent != 0 {
            return Err(
                "--shift-percent is not valid with --link echocat: it keeps SYNC0 at shift 0 \
                 and phase-locks the send instant on its own"
                    .to_string(),
            );
        }
        if self.link != LinkKind::Echocat && self.sleep_strategy != SleepStrategyArg::Sleep {
            return Err(
                "--sleep-strategy is only valid with --link echocat: the other links do not \
                 drive the cycle wait themselves"
                    .to_string(),
            );
        }
        if self.sleep_strategy == SleepStrategyArg::Spin && self.spin_margin.is_zero() {
            return Err(
                "--spin-margin 0s leaves no room for the OS to oversleep past the deadline"
                    .to_string(),
            );
        }
        if self.link == LinkKind::Nop {
            if self.devices.is_none() {
                return Err("--devices is required with --link nop".to_string());
            }
            if self.interface.is_some() {
                return Err("--interface is not valid with --link nop".to_string());
            }
        } else if self.sync0_period.is_zero() {
            return Err("--sync0-period 0ms (free-run) is only valid with --link nop".to_string());
        }
        if let Some(p) = self.rt_priority
            && p > 99
        {
            return Err(format!("--rt-priority {p} must be in 0..=99"));
        }
        Ok(())
    }

    pub fn sync0_shift(&self) -> Duration {
        let nanos = self.sync0_period.as_nanos() * u128::from(self.shift_percent) / 100;
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    pub fn echocat_sleep_strategy(&self) -> SleepStrategy {
        match self.sleep_strategy {
            SleepStrategyArg::Sleep => SleepStrategy::Sleep,
            SleepStrategyArg::Spin => SleepStrategy::Spin {
                margin: self.spin_margin,
            },
        }
    }
}
