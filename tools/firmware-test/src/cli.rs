use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs::{ExecAt, TransportOption};
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
    #[arg(long, value_parser = humantime::parse_duration)]
    pub lost_timeout: Option<Duration>,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "Sends every case frame with ExecAt::Lead of this duration, e.g. 1ms. At least 100us. Omit to process every frame on arrival"
    )]
    pub exec_lead: Option<Duration>,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "Maps to TransportOption::ack_timeout. Must exceed --exec-lead"
    )]
    pub ack_timeout: Option<Duration>,
}

impl Cli {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.heartbeat.is_zero() {
            return Err("--heartbeat must be longer than 0".to_string());
        }
        if self
            .exec_lead
            .is_some_and(|lead| lead < Duration::from_micros(100))
        {
            return Err("--exec-lead must be at least 100us".to_string());
        }
        if self.ack_timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err("--ack-timeout must be longer than 0".to_string());
        }
        let ack_timeout = self.ack_timeout();
        if let ExecAt::Lead(exec_lead) = self.exec()
            && ack_timeout <= exec_lead
        {
            return Err(format!(
                "the ack timeout {ack_timeout:?} must exceed --exec-lead {exec_lead:?}; raise --ack-timeout"
            ));
        }
        Ok(())
    }

    pub(crate) fn ack_timeout(&self) -> Duration {
        self.ack_timeout
            .unwrap_or(TransportOption::default().ack_timeout)
    }

    pub(crate) fn exec(&self) -> ExecAt {
        self.exec_lead.map_or(ExecAt::Immediate, ExecAt::Lead)
    }
}
