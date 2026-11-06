use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{Clear, Pattern, SetSilencer, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, Point3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, ExecAt, Frames, Length, MAX_INFLIGHT};
use autd3_rs::udp::TransportOption;

const NUM_POINTS: usize = 1000;
const RADIUS_MM: f32 = 30.0;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let option = TransportOption::default();
    let client = Client::open(&geometry, &option).await?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;

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

    client.send(Pattern::new(&geometry.phase_buffer(), Intensity::MIN)).await?;
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
    let mut pending: VecDeque<(u16, usize)> = VecDeque::with_capacity(MAX_INFLIGHT);
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
                wait_for_the_oldest(client, &mut pending).await?;
            }
            pending.push_back((client.send_frame(frame, ExecAt::Immediate).await?, 0));
        }
    }
    // Drain the remaining responses.
    while !pending.is_empty() {
        wait_for_the_oldest(client, &mut pending).await?;
    }
    // ANCHOR_END: streaming
    Ok(())
}

async fn wait_for_the_oldest(client: &Client, pending: &mut VecDeque<(u16, usize)>) -> Result<()> {
    while pending.front().is_some_and(|&(_, replies)| replies < client.num_devices()) {
        let reply = client.recv().await?;
        if let Some((_, replies)) = pending.iter_mut().find(|(msg_id, _)| *msg_id == reply.msg_id) {
            anyhow::ensure!(reply.status == 0, "device {} rejected a frame: {:#04x}", reply.device, reply.status);
            *replies += 1;
        }
    }
    pending.pop_front();
    Ok(())
}
