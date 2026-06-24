// Circular foci STM at 1 Hz. Run with: cargo xtask example foci_stm

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, ClientConfig, FociStm, FociStmOption, Silencer, circle};
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

    // 200-point circle of radius 30 mm, 150 mm above the array center.
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let points = circle(center, 30.0 * mm, 200, Vector3::z_axis(), Intensity::MAX);

    let mut builder = client.datagram_builder();
    builder.push(Silencer::default()).push(FociStm::new(
        1.0 * Hz,
        &points,
        FociStmOption::default(),
    ));
    let datagrams = builder.build()?;
    let mut pending = Vec::with_capacity(datagrams.len());
    for frame in &datagrams {
        pending.push(client.send(frame).await?);
    }
    for response in pending {
        response.await?.check()?;
    }

    println!("running a 1 Hz circular foci STM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}
