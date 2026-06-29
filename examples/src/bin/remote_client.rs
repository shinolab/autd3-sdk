// Remote Link client: connects to a remote_server over TCP and emits a 200 Hz
// sine AM focus exactly as a local link would — the Client API is unchanged.
// Start remote_server first. Run with: cargo xtask example remote_client

use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig, Modulation, Pattern, SetSilencer};
use autd3_rs_link_remote::RemoteLinkOption;

const SERVER_ADDR: &str = "127.0.0.1:8080";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let addr: SocketAddr = SERVER_ADDR.parse()?;
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(
        &geometry,
        RemoteLinkOption::new(addr),
        ClientConfig::default(),
    )
    .await?;

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
