use std::net::{IpAddr, SocketAddr};

use autd3_rs_link_twincat::AmsNetId;
use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    #[default]
    Echocat,
    Twincat,
    Remote,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "autd3-rs-firmware-test", about)]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = LinkKind::Echocat)]
    pub link: LinkKind,
    #[arg(long, default_value = None)]
    pub interface: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub devices: usize,
    #[arg(long, default_value_t = 1000)]
    pub cycle_us: u64,
    #[arg(long, default_value_t = default_remote_addr())]
    pub remote_addr: SocketAddr,
    #[arg(long)]
    pub twincat_remote: Option<IpAddr>,
    #[arg(long)]
    pub ams_net_id: Option<AmsNetId>,
}

fn default_remote_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("valid default addr")
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.devices == 0 {
            return Err("--devices must be at least 1".to_string());
        }
        match self.link {
            LinkKind::Twincat => {
                if self.twincat_remote.is_some() && self.ams_net_id.is_none() {
                    return Err("--ams-net-id is required when --twincat-remote is set".to_string());
                }
                if self.interface.is_some() {
                    return Err("--interface is not valid with --link twincat".to_string());
                }
            }
            _ => {
                if self.twincat_remote.is_some() || self.ams_net_id.is_some() {
                    return Err(
                        "--twincat-remote / --ams-net-id are only valid with --link twincat"
                            .to_string(),
                    );
                }
            }
        }
        Ok(())
    }
}
