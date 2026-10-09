// Circular foci STM at 1 Hz.
//
// Run with: cargo xtask rust example foci_stm

use anyhow::Result;

use autd3_rs::commands::{FociStm, FociStmOption, SetSilencer, circle};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, ClientConfig, TransportOption};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(
        &geometry,
        &TransportOption::default(),
        ClientConfig::default(),
    )
    .await?;

    println!("devices: {}", client.num_devices());

    // 200-point circle of radius 30 mm, 150 mm above the array center.
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut points = Vec::new();
    circle(
        center,
        30.0 * mm,
        200,
        Vector3::z_axis(),
        Intensity::MAX,
        &mut points,
    );

    client.send(SetSilencer::default()).await?;
    client
        .send_streaming(FociStm::new(1.0 * Hz, &points, FociStmOption::default()))
        .await?
        .await?;

    println!("running a 1 Hz circular foci STM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.silent_stop().await?;
    client.close().await?;
    Ok(())
}
