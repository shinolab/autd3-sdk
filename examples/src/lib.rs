use std::net::{Ipv6Addr, SocketAddrV6};

use anyhow::{Context, Result};
use autd3_rs::geometry::Geometry;
use autd3_rs::{Interface, TransportOption};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "44336",
        help = "Connect to the simulator on this host, listening on PORT (default 44336): \
                sends the geometry to its control port PORT + 1, then opens Interface::Addr([::1]:PORT)"
    )]
    simulator: Option<u16>,
}

pub fn transport_option(geometry: &Geometry) -> Result<TransportOption> {
    let Some(port) = Args::parse().simulator else {
        return Ok(TransportOption::default());
    };
    let option = TransportOption {
        iface: Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
        ..Default::default()
    };
    autd3_rs_simulator_control::send_geometry(port, geometry, option.open_timeout)
        .context("sending the geometry to the simulator")?;
    Ok(option)
}
