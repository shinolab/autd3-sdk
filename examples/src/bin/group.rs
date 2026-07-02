// Per-device-group command: focus each device group at a different target.
// Run with: cargo xtask example group

use anyhow::Result;

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitQuaternion, offset};
use autd3_rs::units::{m, mm, s};
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

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let focus_option = autd3_rs_pattern::FocusOption::default();

    let left_target = geometry.center() + offset(-40.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut left = geometry.pattern_buffer();
    autd3_rs_pattern::focus(&geometry, left_target, wavelength, &focus_option, &mut left);

    let right_target = geometry.center() + offset(40.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut right = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        right_target,
        wavelength,
        &focus_option,
        &mut right,
    );

    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push_each(|device| {
        Some(if device % 2 == 0 {
            Pattern::new(&left)
        } else {
            Pattern::new(&right)
        })
    });
    let datagrams = builder.build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }

    println!("even devices -> left target, odd devices -> right target — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}
