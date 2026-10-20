// Circular foci STM at 1 Hz.
//
// Run with: cargo xtask rust example foci_stm

use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{Clear, FociStm, FociStmOption, SetSilencer, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::ControlPoints;
use autd3_rs::{Client, TransportOption};
use tracing_subscriber::EnvFilter;

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

    // 200-point circle of radius 30 mm, 150 mm above the array center.
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let points: Vec<ControlPoints<1>> = (0..200)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 200.0;
            ControlPoints::from(
                center + offset(30.0 * mm * theta.cos(), 30.0 * mm * theta.sin(), 0.0 * mm),
            )
        })
        .collect();

    client.send(SetSilencer::default()).await?;
    client
        .send(FociStm::new(1.0 * Hz, &points, FociStmOption::default()))
        .await?;

    println!("running a 1 Hz circular foci STM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.close().await?;
    Ok(())
}
