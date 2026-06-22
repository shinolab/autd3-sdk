use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_IN_FLIGHT;
use autd3_rs::operation::XOR_HASH_MAX_DATA_LEN;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    StopAndWait,
    #[default]
    Streaming,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Ethercrab,
    Soem,
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
}

#[derive(Args, Debug, Clone)]
pub struct MeasureArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(long, default_value_t = 1000)]
    pub cycle_us: u64,

    #[arg(long, default_value_t = 0)]
    pub shift_percent: u8,
}

#[derive(Args, Debug, Clone)]
pub struct TuneArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(long, default_value_t = 500)]
    pub period_min: u64,

    #[arg(long, default_value_t = 2000)]
    pub period_max: u64,

    #[arg(long, default_value_t = 500)]
    pub period_step: u64,

    #[arg(long, default_value_t = 0)]
    pub shift_min: u8,

    #[arg(long, default_value_t = 100)]
    pub shift_max: u8,

    #[arg(long, default_value_t = 25)]
    pub shift_step: u8,
}

#[derive(Args, Debug, Clone)]
pub struct Common {
    #[arg(long, value_enum, default_value_t = LinkKind::Ethercrab)]
    pub link: LinkKind,
    #[arg(long, default_value = None)]
    pub interface: Option<String>,

    #[arg(long)]
    pub devices: Option<usize>,

    #[arg(long, value_enum, default_value_t = Mode::Streaming)]
    pub mode: Mode,
    #[arg(long, default_value_t = MAX_IN_FLIGHT)]
    pub inflight: usize,
    #[arg(long, default_value_t = XOR_HASH_MAX_DATA_LEN)]
    pub data_len: usize,
    #[arg(long, default_value_t = 0)]
    pub sleep_ms: u16,
    #[arg(long, default_value_t = 10)]
    pub timeout_cycles: u32,
    #[arg(long, default_value_t = NonZeroU32::new(1).unwrap())]
    pub send_interval_cycles: NonZeroU32,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap())]
    pub max_resync_rounds: NonZeroU32,
    #[arg(long, default_value_t = false)]
    pub low_latency: bool,
    #[arg(long, default_value_t = false)]
    pub no_win_perf_tune: bool,
    #[arg(long)]
    pub rt_priority: Option<u8>,
    #[arg(long)]
    pub rt_core: Option<usize>,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "10s")]
    pub dwell: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "2s")]
    pub warmup: Duration,

    #[arg(long, value_parser = humantime::parse_duration, default_value = "100ms")]
    pub poll_interval: Duration,

    #[arg(long)]
    pub csv: Option<PathBuf>,
}

impl Common {
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
        if let Some(p) = self.rt_priority
            && p > 99
        {
            return Err(format!("--rt-priority {p} must be in 0..=99"));
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
        if self.cycle_us == 0 {
            return Err("--cycle-us must be greater than zero".to_string());
        }
        if self.shift_percent > 100 {
            return Err(format!(
                "--shift-percent {} must be in 0..=100",
                self.shift_percent
            ));
        }
        Ok(())
    }
}

impl TuneArgs {
    pub fn validate(&self) -> Result<(), String> {
        self.common.validate()?;
        if self.period_min == 0 {
            return Err("--period-min must be greater than zero".to_string());
        }
        if self.period_min > self.period_max {
            return Err(format!(
                "--period-min {} must be <= --period-max {}",
                self.period_min, self.period_max
            ));
        }
        if self.period_step == 0 {
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
        Ok(())
    }
}
