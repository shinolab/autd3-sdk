use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, Point3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::{Client, ClientConfig, Length, MAX_IN_FLIGHT, ResponseFuture};
use autd3_rs_link_nop::Nop;

const NUM_POINTS: usize = 1000;
const RADIUS_MM: f32 = 30.0;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default());
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: targets
    // Prepare 1000 focus points along a circle 150 mm above the array center.
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let targets: Vec<Point3<f32>> = (0..NUM_POINTS)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / NUM_POINTS as f32;
            center
                + offset(
                    RADIUS_MM * theta.cos() * mm,
                    RADIUS_MM * theta.sin() * mm,
                    0.0 * mm,
                )
        })
        .collect();
    // ANCHOR_END: targets

    stop_and_wait(&client, &geometry, &targets, wavelength).await?;
    streaming(&client, &geometry, &targets, wavelength).await?;

    client.stop().await?;
    client.close().await?;
    Ok(())
}

async fn stop_and_wait(
    client: &Client,
    geometry: &Geometry,
    targets: &[Point3<f32>],
    wavelength: Length,
) -> Result<()> {
    // ANCHOR: stop_and_wait
    let mut patterns = geometry.pattern_buffer();
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &autd3_rs_pattern::FocusOption::default(),
            &mut patterns,
        );
        let mut builder = client.datagram_builder();
        builder.push(Pattern::new(&patterns));
        for frame in &builder.build()? {
            client.send_checked(frame).await?;
        }
    }
    // ANCHOR_END: stop_and_wait
    Ok(())
}

async fn streaming(
    client: &Client,
    geometry: &Geometry,
    targets: &[Point3<f32>],
    wavelength: Length,
) -> Result<()> {
    // ANCHOR: streaming
    let mut patterns = geometry.pattern_buffer();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(MAX_IN_FLIGHT);
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &autd3_rs_pattern::FocusOption::default(),
            &mut patterns,
        );
        let mut builder = client.datagram_builder();
        builder.push(Pattern::new(&patterns));
        for frame in &builder.build()? {
            if pending.len() >= MAX_IN_FLIGHT {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send(frame).await?);
        }
    }
    // Drain the remaining responses.
    while let Some(fut) = pending.pop_front() {
        fut.await?.check()?;
    }
    // ANCHOR_END: streaming
    Ok(())
}
