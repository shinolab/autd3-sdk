use std::net::{Ipv6Addr, SocketAddrV6};

use autd3_rs::{Interface, TransportOption};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "44336",
        help = "Connect to the simulator on this host, listening on PORT (default 44336), with Interface::Addr([::1]:PORT). \
                Set the same number and placement of devices in the Geometry tab of the simulator first"
    )]
    simulator: Option<u16>,
}

#[must_use]
pub fn transport_option() -> TransportOption {
    match Args::parse().simulator {
        Some(port) => TransportOption {
            iface: Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
            ..Default::default()
        },
        None => TransportOption::default(),
    }
}
