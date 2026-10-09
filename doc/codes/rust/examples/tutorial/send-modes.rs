use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, Point3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, ClientConfig, Frames, Length, MAX_INFLIGHT, ResponseFuture};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

const NUM_POINTS: usize = 1000;
const RADIUS_MM: f32 = 30.0;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    client.send(SetSilencer::default()).await?;

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

    client.silent_stop().await?;
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
    let mut phases = geometry.phase_buffer();
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &mut phases,
        );
        client.send(Pattern::new(&phases, Intensity::MAX)).await?;
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
    let mut phases = geometry.phase_buffer();
    let mut frames = Frames::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(MAX_INFLIGHT);
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &mut phases,
        );
        frames.encode_into(geometry, Pattern::new(&phases, Intensity::MAX))?;
        for frame in &frames {
            if pending.len() >= MAX_INFLIGHT {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send_frame(frame).await?);
        }
    }
    // Drain the remaining responses.
    while let Some(response) = pending.pop_front() {
        response.await?.check()?;
    }
    // ANCHOR_END: streaming
    Ok(())
}
