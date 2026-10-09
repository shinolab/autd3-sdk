use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use autd3_rs::MAX_INFLIGHT;
use autd3_rs::udp::DEVICE_QUEUE_FRAMES;
use clap::{ArgGroup, Parser, ValueEnum};

const DEFAULT_MAX_SAMPLES: u64 = 1_000_000;
const DEFAULT_HEARTBEAT: &str = "10ms";
const DEFAULT_ACK_TIMEOUT: &str = "10ms";

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
        conflicts_with_all = ["interface", "simulator"],
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
                pattern is the production write + config + bank-activation (3 frames, 1 CTL_FLAG latch), \
                write-modulation-buffer writes the whole modulation buffer of bank 1 per sample \
                (its frames pipelined up to --max-inflight; stop-and-wait only)."
    )]
    pub command: Command,
    #[arg(
        long,
        help = "Network interface the devices hang off (maps to TransportOption.iface). \
                Omit to pick the one whose devices answer."
    )]
    pub interface: Option<String>,
    #[arg(
        long,
        conflicts_with = "interface",
        help = "Connect to the simulator on this host only (maps to Interface::Simulator). \
                Without it the simulator is still preferred when it runs."
    )]
    pub simulator: bool,
    #[arg(
        long,
        default_value_t = NonZeroUsize::MIN,
        help = "Device count of the geometry. Opening fails when it does not match the chain."
    )]
    pub devices: NonZeroUsize,
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
    #[arg(
        long = "send-rate-limit",
        help = "Cap on the line occupancy of the sends in percent of the device link speed, e.g. 95 \
                (maps to TransportOption.send_rate_limit). Omit to send without a limit."
    )]
    pub send_rate_limit: Option<f32>,
    #[arg(
        long = "send-buffer",
        help = "Socket send buffer in bytes (maps to TransportOption.send_buffer). 0 keeps the OS \
                default. Omit to use the library default."
    )]
    pub send_buffer: Option<usize>,
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
        conflicts_with = "gpio_base_signal",
        help = "Emit Sync (high on the FPGA clock that detects the sync input edge) on GPIO[0] to probe the sync pulse itself on a scope"
    )]
    pub gpio_sync: bool,
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
        help = "Pipeline depth in streaming mode (maps to ClientConfig.max_inflight). \
                Stop-and-wait ignores it except with --command write-modulation-buffer, \
                which pipelines the frames of one sample up to this depth."
    )]
    pub max_inflight: usize,
    #[arg(long, default_value_t = NonZeroU32::new(8).unwrap(), help = "maps to ClientConfig.max_resync_rounds")]
    pub max_resync_rounds: NonZeroU32,
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
        if self.heartbeat.is_zero() {
            return Err("--heartbeat must be longer than 0".to_string());
        }
        if self.ack_timeout.is_zero() {
            return Err("--ack-timeout must be longer than 0".to_string());
        }
        Ok(())
    }
}
