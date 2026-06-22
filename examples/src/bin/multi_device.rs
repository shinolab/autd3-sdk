// Multiple AUTD3 devices arranged side by side. Run with: cargo xtask example multi_device

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![
        Autd3::new(Point3::origin(), UnitQuaternion::identity()),
        Autd3::new(
            Point3::new(Autd3::DEVICE_WIDTH, 0.0, 0.0),
            UnitQuaternion::identity(),
        ),
    ]);

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

    let center = geometry.center();
    println!(
        "array center: ({:.2}, {:.2}, {:.2}) mm",
        center.x, center.y, center.z
    );

    client.close().await?;
    Ok(())
}
