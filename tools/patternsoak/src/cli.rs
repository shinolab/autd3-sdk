use std::num::NonZeroU32;
use std::time::Duration;

use autd3_rs::MAX_IN_FLIGHT;
use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Ethercrab,
    Soem,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    StopAndWait,
    Streaming,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "autd3-rs-patternsoak", about)]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = LinkKind::Ethercrab)]
    pub link: LinkKind,
    #[arg(long, default_value = None)]
    pub interface: Option<String>,
    #[arg(long)]
    pub devices: Option<usize>,
    #[arg(long, default_value_t = 1000)]
    pub cycle_us: u64,
    #[arg(long)]
    pub count: Option<u64>,
    #[arg(long, value_parser = humantime::parse_duration)]
    pub duration: Option<Duration>,
    #[arg(long, value_enum, default_value_t = Mode::StopAndWait)]
    pub mode: Mode,
    #[arg(long, default_value_t = MAX_IN_FLIGHT)]
    pub inflight: usize,
    #[arg(long, default_value_t = false)]
    pub stop_on_error: bool,
    #[arg(long, default_value_t = false)]
    pub low_latency: bool,
    #[arg(long, default_value_t = 10)]
    pub timeout_cycles: u32,
    #[arg(long, default_value_t = NonZeroU32::new(1).unwrap())]
    pub send_interval_cycles: NonZeroU32,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap())]
    pub max_resync_rounds: NonZeroU32,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Streaming && (self.inflight == 0 || self.inflight > MAX_IN_FLIGHT) {
            return Err(format!(
                "--inflight {} must be in 1..={MAX_IN_FLIGHT}",
                self.inflight,
            ));
        }
        Ok(())
    }
}
