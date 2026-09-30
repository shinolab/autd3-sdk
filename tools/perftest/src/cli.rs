use std::net::SocketAddrV6;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_INFLIGHT;
use autd3_rs::udp::DEVICE_QUEUE_FRAMES;
use clap::{ArgGroup, Parser, ValueEnum};

pub const DEFAULT_MAX_SAMPLES: u64 = 1_000_000;
pub const DEFAULT_HEARTBEAT: &str = "10ms";
pub const DEFAULT_ACK_TIMEOUT: &str = "10ms";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    StopAndWait,
    Streaming,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Command {
    Nop,
    WritePatternBuffer,
    #[default]
    Pattern,
    WriteModulationBuffer,
}

impl Command {
    pub const fn is_pattern(self) -> bool {
        matches!(self, Self::WritePatternBuffer | Self::Pattern)
    }

    pub const fn is_bulk(self) -> bool {
        matches!(self, Self::WriteModulationBuffer)
    }
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
    #[arg(
        long,
        default_value_t = false,
        help = "Run against the in-process UDP device emulator instead of real devices \
                (baseline of the host side; no hardware needed)."
    )]
    pub emulator: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = Command::Pattern,
        help = "Command to measure. nop touches no FPGA register (pure communication path), \
                write-pattern-buffer writes FPGA RAM without latching, \
                pattern is the fused write+config+bank-change that latches CTL_FLAG once per frame, \
                write-modulation-buffer writes the whole modulation buffer of bank 1 per sample \
                (its frames pipelined up to --max-inflight; stop-and-wait only)."
    )]
    pub command: Command,
    #[arg(
        long,
        default_value = None,
        help = "Network interface the devices hang off (maps to TransportOption.iface). \
                Omit to pick the one whose devices answer."
    )]
    pub interface: Option<String>,
    #[arg(
        long,
        help = "Send the multicast management messages here instead of ff02::1 \
                (maps to TransportOption.group), e.g. the simulator's [::1]:44336."
    )]
    pub group: Option<SocketAddrV6>,
    #[arg(
        long,
        default_value_t = 1,
        help = "Device count of the geometry. Opening fails when it does not match the chain."
    )]
    pub devices: usize,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = DEFAULT_HEARTBEAT,
        help = "Heartbeat interval while nothing is sent, e.g. 10ms (maps to TransportOption.heartbeat)."
    )]
    pub heartbeat: Duration,
    #[arg(
        long = "reply-timeout",
        value_parser = humantime::parse_duration,
        help = "How long a heartbeat waits for every reply (maps to TransportOption.reply_timeout). \
                Omit to keep the library default."
    )]
    pub reply_timeout: Option<Duration>,
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
    #[arg(
        long,
        default_value_t = false,
        help = "Read the firmware telemetry counters before and after the run and print the \
                per-device deltas (e.g. Failsafe, SyncResync, FifoDrop)."
    )]
    pub telemetry: bool,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "Keep the connection idle (heartbeats only) this long before measuring, e.g. 3s."
    )]
    pub hold: Option<Duration>,
    #[arg(long)]
    pub csv: Option<PathBuf>,
    #[arg(
        long = "ack-timeout",
        value_parser = humantime::parse_duration,
        default_value = DEFAULT_ACK_TIMEOUT,
        help = "maps to ClientConfig.ack_timeout"
    )]
    pub ack_timeout: Duration,
    #[arg(long, value_enum, default_value_t = Mode::StopAndWait)]
    pub mode: Mode,
    #[arg(
        long = "max-inflight",
        alias = "inflight",
        default_value_t = DEVICE_QUEUE_FRAMES,
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
    #[arg(
        long,
        help = "Run the driver thread at this priority (0..=99). Omit to run it at the normal \
                priority, which is what an application gets by default."
    )]
    pub rt_priority: Option<u8>,
    #[arg(
        long,
        value_enum,
        default_value_t = RtPolicy::Fifo,
        help = "Scheduling policy of the driver thread when --rt-priority is given (Linux)."
    )]
    pub rt_policy: RtPolicy,
    #[arg(
        long = "rt-affinity",
        alias = "rt-core",
        help = "Pin the driver thread to this CPU core."
    )]
    pub rt_affinity: Option<usize>,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "Run the driver as a `poll` loop that sleeps this long between polls, instead of `Driver::run`."
    )]
    pub poll_sleep: Option<Duration>,
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
        if self.command.is_bulk() && self.mode == Mode::Streaming {
            return Err("--command write-modulation-buffer runs in stop-and-wait only".to_string());
        }
        if self.devices == 0 {
            return Err("--devices must be at least 1".to_string());
        }
        if self.interface.is_some() && self.group.is_some() {
            return Err("--interface and --group are mutually exclusive".to_string());
        }
        if self.emulator && (self.interface.is_some() || self.group.is_some()) {
            return Err("--interface / --group are not valid with --emulator".to_string());
        }
        if self.heartbeat.is_zero() {
            return Err("--heartbeat must be longer than 0".to_string());
        }
        if self.ack_timeout.is_zero() {
            return Err("--ack-timeout must be longer than 0".to_string());
        }
        if let Some(p) = self.rt_priority
            && p > 99
        {
            return Err(format!("--rt-priority {p} must be in 0..=99"));
        }
        Ok(())
    }
}
