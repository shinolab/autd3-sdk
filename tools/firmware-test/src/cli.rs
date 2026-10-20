use std::time::Duration;

use autd3_rs::ClientConfig;
use clap::Parser;

#[derive(Parser, Debug, Clone)]
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
        help = "Connect to the simulator on this host only, listening on PORT (default 44336)"
    )]
    pub simulator: Option<u16>,
    #[arg(long, default_value_t = 1)]
    pub devices: usize,
    #[arg(long, default_value_t = 10_000)]
    pub heartbeat_us: u64,
    #[arg(long)]
    pub lost_timeout_ms: Option<u64>,
    #[arg(
        long,
        help = "Maps to ClientConfig::exec_lead. At least 100. Omit to process every frame on arrival"
    )]
    pub exec_lead_us: Option<u64>,
    #[arg(
        long,
        help = "Maps to ClientConfig::ack_timeout. Must exceed --exec-lead-us"
    )]
    pub ack_timeout_ms: Option<u64>,
    #[arg(
        long,
        default_value_t = false,
        help = "Zero the pulse width table after opening and after every case so that no transducer emits"
    )]
    pub mute: bool,
}

impl Cli {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.devices == 0 {
            return Err("--devices must be at least 1".to_string());
        }
        if self.heartbeat_us == 0 {
            return Err("--heartbeat-us must be at least 1".to_string());
        }
        if self.exec_lead_us.is_some_and(|us| us < 100) {
            return Err("--exec-lead-us must be at least 100".to_string());
        }
        if self.ack_timeout_ms == Some(0) {
            return Err("--ack-timeout-ms must be at least 1".to_string());
        }
        let config = self.client_config();
        if let Some(exec_lead) = config.exec_lead
            && config.ack_timeout <= exec_lead
        {
            return Err(format!(
                "the ack timeout {:?} must exceed --exec-lead-us {exec_lead:?}; raise --ack-timeout-ms",
                config.ack_timeout
            ));
        }
        Ok(())
    }

    pub(crate) fn client_config(&self) -> ClientConfig {
        let default = ClientConfig::default();
        ClientConfig {
            ack_timeout: self
                .ack_timeout_ms
                .map_or(default.ack_timeout, Duration::from_millis),
            exec_lead: self.exec_lead_us.map(Duration::from_micros),
            ..default
        }
    }
}
