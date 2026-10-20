// Pattern STM: a circle of host-computed focus patterns played back at 1 Hz.
//
// Run with: cargo xtask rust example pattern_stm

use anyhow::Result;

use autd3_rs::commands::{Clear, PatternStm, PatternStmOption, SetSilencer, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, TransportOption};
use tracing_subscriber::EnvFilter;

const NUM_POINTS: usize = 200;
const RADIUS_MM: f32 = 30.0;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, &TransportOption::default())?;
    client.send((Clear, Synchronize)).await?;

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
            let mut phases = geometry.phase_buffer();
            autd3_rs_pattern::focus(&geometry, target, wavelength, &mut phases);
            phases
        })
        .collect::<Vec<_>>();

    client.send(SetSilencer::default()).await?;
    client
        .send(PatternStm::new(
            1.0 * Hz,
            &patterns,
            Intensity::MAX,
            PatternStmOption::default(),
        ))
        .await?;

    println!("running a 1 Hz circular pattern STM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.close().await?;
    Ok(())
}
