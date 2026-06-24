// Single focus with a 200 Hz sine AM. Run with: cargo xtask example focus_sine

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig, Modulation, Pattern, SetSilencer};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(
        &geometry,
        EtherCrabLinkOption::default(),
        ClientConfig::default(),
    )
    .await?;

    println!("devices: {}", client.num_devices());
    for (i, fw) in client.read_firmware_version().await?.iter().enumerate() {
        println!("device[{i}] firmware version: {fw}");
    }

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut patterns = client.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut patterns,
    );

    let mut modulation = client.modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&patterns))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    let datagrams = builder.build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }

    println!(
        "emitting a 200 Hz AM focus at ({:.2}, {:.2}, {:.2}) mm — press Ctrl+C to stop",
        target.x, target.y, target.z
    );
    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}
