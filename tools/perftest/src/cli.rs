use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs::MAX_INFLIGHT;
use clap::{Parser, ValueEnum};

#[derive(Clone, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum Mode {
    #[default]
    StopAndWait,
    Streaming,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum Command {
    Nop,
    WritePatternBuffer,
    #[default]
    Pattern,
}

impl Command {
    pub(crate) const fn is_pattern(self) -> bool {
        matches!(self, Self::WritePatternBuffer | Self::Pattern)
    }
}

#[derive(Parser, Clone)]
#[command(name = "autd3-rs-perftest", about)]
pub(crate) struct Cli {
    #[arg(
        long,
        value_enum,
        default_value_t = Command::Pattern,
        help = "Command to measure. nop touches no FPGA register (pure communication path), \
                write-pattern-buffer writes FPGA RAM without latching, \
                pattern is the production write + config + bank-activation (1 fused frame, 1 CTL_FLAG latch)."
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
        default_value_t = NonZeroUsize::MIN,
        help = "Device count of the geometry. Opening fails when it does not match the chain."
    )]
    pub devices: NonZeroUsize,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = "10ms",
        help = "Interval of Controller::send_heartbeat while --hold keeps the connection idle, e.g. 10ms."
    )]
    pub heartbeat: Duration,
    #[arg(
        long = "send-buffer",
        help = "Socket send buffer in bytes (maps to TransportOption.send_buffer_bytes). 0 keeps the OS \
                default. Omit to use the library default."
    )]
    pub send_buffer: Option<usize>,
    #[arg(
        long,
        default_value_t = false,
        help = "Stop at the first failed send and exit non-zero (soak testing). \
                The summary is still printed."
    )]
    pub stop_on_error: bool,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "Keep the connection idle (heartbeats only) this long before measuring, e.g. 3s."
    )]
    pub hold: Option<Duration>,
    #[arg(
        long = "ack-timeout",
        value_parser = humantime::parse_duration,
        default_value = "10ms",
        help = "How long to wait for the replies of a frame before counting it as a timeout"
    )]
    pub ack_timeout: Duration,
    #[arg(
        long,
        value_enum,
        default_value_t = Mode::StopAndWait,
        help = "stop-and-wait or streaming."
    )]
    pub mode: Mode,
    #[arg(
        long = "max-inflight",
        alias = "inflight",
        default_value_t = MAX_INFLIGHT,
        help = "Pipeline depth in streaming mode: frames sent before awaiting the oldest response. \
                Stop-and-wait ignores it."
    )]
    pub max_inflight: usize,
}

impl Cli {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Streaming
            && (self.max_inflight == 0 || self.max_inflight > MAX_INFLIGHT)
        {
            return Err(format!(
                "--max-inflight {} must be in 1..={MAX_INFLIGHT}",
                self.max_inflight,
            ));
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
