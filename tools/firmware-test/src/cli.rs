use std::num::NonZeroUsize;
use std::time::Duration;

use clap::Parser;

#[derive(Parser)]
#[command(name = "autd3-rs-firmware-test", about)]
pub(crate) struct Cli {
    #[arg(long, default_value = None)]
    pub interface: Option<String>,
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "44336",
        conflicts_with = "interface",
        help = "Connect to the simulator on this host, listening on PORT (default 44336), \
                by opening Interface::Addr([::1]:PORT). Set the geometry in the simulator's Geometry tab first."
    )]
    pub simulator: Option<u16>,
    #[arg(long, default_value_t = NonZeroUsize::MIN)]
    pub devices: NonZeroUsize,
    #[arg(long, value_parser = humantime::parse_duration, default_value = "10ms")]
    pub heartbeat: Duration,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = "200ms",
        help = "How long to wait for the replies of a frame"
    )]
    pub ack_timeout: Duration,
}

impl Cli {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.heartbeat.is_zero() {
            return Err("--heartbeat must be longer than 0".to_string());
        }
        if self.ack_timeout.is_zero() {
            return Err("--ack-timeout must be longer than 0".to_string());
        }
        Ok(())
    }
}
