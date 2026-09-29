use std::net::SocketAddrV6;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "autd3-rs-firmware-test", about)]
pub struct Cli {
    #[arg(long, default_value = None)]
    pub interface: Option<String>,
    #[arg(long, conflicts_with = "interface")]
    pub group: Option<SocketAddrV6>,
    #[arg(long, default_value_t = 1)]
    pub devices: usize,
    #[arg(long, default_value_t = 10_000)]
    pub heartbeat_us: u64,
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
