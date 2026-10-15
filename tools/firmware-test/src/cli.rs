use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "autd3-rs-firmware-test", about)]
pub struct Cli {
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
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.devices == 0 {
            return Err("--devices must be at least 1".to_string());
        }
        if self.heartbeat_us == 0 {
            return Err("--heartbeat-us must be at least 1".to_string());
        }
        Ok(())
    }
}
