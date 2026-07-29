use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_INFLIGHT;
use clap::{Args, Parser, Subcommand, ValueEnum};

const ECHOCAT_SHIFT_ERR: &str = "a SYNC0 shift is not valid with --link echocat: it keeps SYNC0 \
                                 at shift 0 and phase-locks the send instant on its own";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    StopAndWait,
    #[default]
    Streaming,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Echocat,
    Ethercrab,
    Soem,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum RtPolicy {
    Normal,
    #[default]
    Fifo,
    RoundRobin,
}

#[derive(Parser, Debug)]
#[command(name = "autd3-rs-synctune", about)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Measure(MeasureArgs),

    Tune(TuneArgs),

    Drift(DriftArgs),
}

#[derive(Args, Debug, Clone)]
pub struct MeasureArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(
        long = "sync0-period",
        value_parser = humantime::parse_duration,
        default_value = "1ms",
        help = "SYNC0 / EtherCAT cycle period, e.g. 1ms / 500us (maps to *LinkOption.sync0_period)."
    )]
    pub sync0_period: Duration,

    #[arg(
        long,
        default_value_t = 0,
        help = "SYNC0 shift as a percent of the period (maps to *LinkOption.sync0_shift = period * percent)."
    )]
    pub shift_percent: u8,
}

#[derive(Args, Debug, Clone)]
pub struct DriftArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(
        long = "sync0-period",
        value_parser = humantime::parse_duration,
        default_value = "1ms",
        help = "SYNC0 / EtherCAT cycle period, e.g. 1ms / 500us (maps to *LinkOption.sync0_period)."
    )]
    pub sync0_period: Duration,

    #[arg(
        long,
        default_value_t = 0,
        help = "SYNC0 shift as a percent of the period (maps to *LinkOption.sync0_shift = period * percent)."
    )]
    pub shift_percent: u8,

    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = "120s",
        help = "Sampling window. Overrides --dwell; longer windows tighten the ppm estimate."
    )]
    pub duration: Duration,
}

#[derive(Args, Debug, Clone)]
pub struct TuneArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "1ms")]
    pub period_min: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "2ms")]
    pub period_max: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "1ms")]
    pub period_step: Duration,

    #[arg(long, default_value_t = 0)]
    pub shift_min: u8,

    #[arg(long, default_value_t = 100)]
    pub shift_max: u8,

    #[arg(long, default_value_t = 50)]
    pub shift_step: u8,
}

#[derive(Args, Debug, Clone)]
pub struct Common {
    #[arg(long, value_enum, default_value_t = LinkKind::Echocat)]
    pub link: LinkKind,
    #[arg(
        long,
        default_value = None,
        help = "EtherCAT network interface (maps to *LinkOption.iface)."
    )]
    pub interface: Option<String>,

    #[arg(long)]
    pub devices: Option<usize>,

    #[arg(long, value_enum, default_value_t = Mode::Streaming)]
    pub mode: Mode,
    #[arg(
        long = "max-inflight",
        alias = "inflight",
        default_value_t = MAX_INFLIGHT,
        help = "Pipeline depth in streaming mode (maps to ClientConfig.max_inflight)."
    )]
    pub max_inflight: usize,
    #[arg(
        long,
        default_value_t = 10,
        help = "maps to ClientConfig.timeout_cycles"
    )]
    pub timeout_cycles: u32,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap(), help = "maps to ClientConfig.max_resync_rounds")]
    pub max_resync_rounds: NonZeroU32,
    #[arg(
        long,
        default_value_t = false,
        help = "maps to ClientConfig.low_latency"
    )]
    pub low_latency: bool,
    #[arg(long, default_value_t = false)]
    pub no_win_perf_tune: bool,
    #[arg(
        long,
        help = "maps to ClientConfig.rt_priority (0..=99). Omit to keep the library default \
                (TimeCritical on Windows, SCHED_FIFO 80 elsewhere)."
    )]
    pub rt_priority: Option<u8>,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "rt_priority",
        help = "Force ClientConfig.rt_priority = None (no RT scheduling), overriding the library \
                default. Use to compare against the pre-default behaviour."
    )]
    pub no_rt_priority: bool,
    #[arg(long, value_enum, default_value_t = RtPolicy::Fifo, help = "maps to ClientConfig.rt_policy")]
    pub rt_policy: RtPolicy,
    #[arg(
        long = "rt-affinity",
        alias = "rt-core",
        help = "Pin the RT thread to this CPU core (maps to ClientConfig.rt_affinity)."
    )]
    pub rt_affinity: Option<usize>,

    #[arg(
        long,
        help = "--link ethercrab only: maps to EtherCrabLinkOptionFull.tx_rx_priority (0..=99). \
                Omit to keep the library default (90 outside Windows)."
    )]
    pub tx_rx_priority: Option<u8>,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "tx_rx_priority",
        help = "--link ethercrab only: force tx_rx_priority = None (pump thread left at OS default)."
    )]
    pub no_tx_rx_priority: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = RtPolicy::Fifo,
        help = "--link ethercrab only: maps to EtherCrabLinkOptionFull.tx_rx_policy"
    )]
    pub tx_rx_policy: RtPolicy,
    #[arg(
        long,
        help = "--link ethercrab only: pin the tx/rx pump thread to this CPU core."
    )]
    pub tx_rx_affinity: Option<usize>,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "30s")]
    pub dwell: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "5s")]
    pub warmup: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "100ms")]
    pub poll_interval: Duration,

    #[arg(long)]
    pub csv: Option<PathBuf>,
}

impl Common {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Streaming
            && (self.max_inflight == 0 || self.max_inflight > MAX_INFLIGHT)
        {
            return Err(format!(
                "--max-inflight {} must be in 1..={MAX_INFLIGHT}",
                self.max_inflight,
            ));
        }
        if let Some(p) = self.rt_priority
            && p > 99
        {
            return Err(format!("--rt-priority {p} must be in 0..=99"));
        }
        if let Some(p) = self.tx_rx_priority
            && p > 99
        {
            return Err(format!("--tx-rx-priority {p} must be in 0..=99"));
        }
        if self.poll_interval.is_zero() {
            return Err("--poll-interval must be greater than zero".to_string());
        }
        if self.dwell.is_zero() {
            return Err("--dwell must be greater than zero".to_string());
        }
        Ok(())
    }
}

impl MeasureArgs {
    pub fn validate(&self) -> Result<(), String> {
        self.common.validate()?;
        if self.sync0_period.is_zero() {
            return Err("--sync0-period must be greater than zero".to_string());
        }
        if self.shift_percent > 100 {
            return Err(format!(
                "--shift-percent {} must be in 0..=100",
                self.shift_percent
            ));
        }
        if self.common.link == LinkKind::Echocat && self.shift_percent != 0 {
            return Err(ECHOCAT_SHIFT_ERR.to_string());
        }
        Ok(())
    }
}

impl DriftArgs {
    pub fn validate(&self) -> Result<(), String> {
        self.common.validate()?;
        if self.sync0_period.is_zero() {
            return Err("--sync0-period must be greater than zero".to_string());
        }
        if self.shift_percent > 100 {
            return Err(format!(
                "--shift-percent {} must be in 0..=100",
                self.shift_percent
            ));
        }
        if self.common.link == LinkKind::Echocat && self.shift_percent != 0 {
            return Err(ECHOCAT_SHIFT_ERR.to_string());
        }
        if self.duration.is_zero() {
            return Err("--duration must be greater than zero".to_string());
        }
        Ok(())
    }

    #[must_use]
    pub fn common(&self) -> Common {
        Common {
            dwell: self.duration,
            ..self.common.clone()
        }
    }
}

impl TuneArgs {
    pub fn validate(&self) -> Result<(), String> {
        self.common.validate()?;
        if self.period_min.is_zero() {
            return Err("--period-min must be greater than zero".to_string());
        }
        if self.period_min > self.period_max {
            return Err(format!(
                "--period-min {:?} must be <= --period-max {:?}",
                self.period_min, self.period_max
            ));
        }
        if self.period_step.is_zero() {
            return Err("--period-step must be greater than zero".to_string());
        }
        if self.shift_max > 100 {
            return Err(format!("--shift-max {} must be in 0..=100", self.shift_max));
        }
        if self.shift_min > self.shift_max {
            return Err(format!(
                "--shift-min {} must be <= --shift-max {}",
                self.shift_min, self.shift_max
            ));
        }
        if self.shift_step == 0 {
            return Err("--shift-step must be greater than zero".to_string());
        }
        if self.common.link == LinkKind::Echocat && self.shift_max != 0 {
            return Err(ECHOCAT_SHIFT_ERR.to_string());
        }
        Ok(())
    }
}
