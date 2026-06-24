// Gain (pattern) STM: a circle of host-computed focus patterns played back at 1 Hz.
// Run with: cargo xtask example pattern_stm

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::{Client, ClientConfig, GainStm, GainStmMode, GainStmOption, Silencer};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

const NUM_POINTS: usize = 200;
const RADIUS_MM: f32 = 30.0;

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

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let patterns = (0..NUM_POINTS)
        .map(|i| {
            let theta = 2.0 * std::f32::consts::PI * i as f32 / NUM_POINTS as f32;
            let target = center
                + offset(
                    RADIUS_MM * theta.cos() * mm,
                    RADIUS_MM * theta.sin() * mm,
                    0.0 * mm,
                );
            let mut buffer = client.pattern_buffer();
            autd3_rs_pattern::focus(
                &geometry,
                target,
                wavelength,
                &autd3_rs_pattern::FocusOption::default(),
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();

    let mut builder = client.datagram_builder();
    builder.push(Silencer::default()).push(GainStm::new(
        1.0 * Hz,
        &patterns,
        GainStmOption {
            mode: GainStmMode::PhaseFull,
            ..Default::default()
        },
    ));
    let datagrams = builder.build()?;
    let mut pending = Vec::with_capacity(datagrams.len());
    for frame in &datagrams {
        pending.push(client.send(frame).await?);
    }
    for response in pending {
        response.await?.check()?;
    }

    println!("running a 1 Hz circular pattern (gain) STM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}
