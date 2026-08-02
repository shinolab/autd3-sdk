// Remote Link client: connects to a remote_server over TCP and emits a 200 Hz sine AM focus.
// Start remote_server (or the simulator) first.
// Pass an address to skip the mDNS lookup; without one it falls back to the local default
// when no appliance answers, which is where remote_server and the simulator both listen.
//
// Run with: cargo xtask example remote_client

use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_remote::RemoteLinkOption;

const LOCAL_ADDR: &str = "127.0.0.1:8080";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let option = match std::env::args().nth(1) {
        Some(addr) => RemoteLinkOption::new(addr.parse()?),
        None => match RemoteLinkOption::discover() {
            Ok(option) => option,
            Err(e) => {
                println!("discovery found no appliance ({e}); falling back to {LOCAL_ADDR}");
                RemoteLinkOption::new(LOCAL_ADDR.parse::<SocketAddr>()?)
            }
        },
    };
    let addr = option.addr;
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, option, ClientConfig::default()).await?;

    println!("connected to {addr}, devices: {}", client.num_devices());
    for (i, fw) in client.read_firmware_version().await?.iter().enumerate() {
        println!("device[{i}] firmware version: {fw}");
    }

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut emissions = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut emissions,
    );

    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&emissions))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    let datagrams = builder.build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }

    println!("emitting a 200 Hz AM focus over the network — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}
