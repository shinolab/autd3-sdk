// Sweeps a focus around a circle, sending the same update sequence two ways to show how the send loop chooses its mode.
//
// Run with: cargo xtask rust example send_modes

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

use anyhow::Result;

use autd3_rs::commands::{ConfigPattern, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry, Point3, offset};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig};
use autd3_rs::{
    Client, ClientConfig, Frames, Length, MAX_INFLIGHT, ResponseFuture, TransportOption,
};

const TOTAL_POINTS: usize = 1000;

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

    configure(&client).await?;

    let center = geometry.center();
    let radius = 30.0 * mm;
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let targets: Vec<Point3<f32>> = (0..TOTAL_POINTS)
        .map(|i| {
            let theta = 2. * PI * (i as f32) / (TOTAL_POINTS as f32);
            center + offset(radius * theta.cos(), radius * theta.sin(), 150.0 * mm)
        })
        .collect();

    println!("sweeping a focus through {TOTAL_POINTS} positions, twice");

    let elapsed = run_stop_and_wait(&client, &targets, wavelength).await?;
    report("stop-and-wait", elapsed);

    let elapsed = run_streaming(&client, &targets, wavelength, MAX_INFLIGHT).await?;
    report("streaming", elapsed);

    client.silent_stop().await?;
    client.close().await?;

    Ok(())
}

// One round-trip per frame: confirm each update lands before issuing the next.
async fn run_stop_and_wait(
    client: &Client,
    targets: &[Point3<f32>],
    wavelength: Length,
) -> Result<Duration> {
    let geometry = client.geometry();
    let mut phases = geometry.phase_buffer();

    let start = Instant::now();
    for &target in targets {
        autd3_rs_pattern::focus(geometry, target, wavelength, &mut phases);
        client.send(write_focus(&phases)).await?;
    }
    Ok(start.elapsed())
}

// Keep `max_inflight` frames on the wire; drain responses behind the send cursor.
async fn run_streaming(
    client: &Client,
    targets: &[Point3<f32>],
    wavelength: Length,
    max_inflight: usize,
) -> Result<Duration> {
    let geometry = client.geometry();
    let mut phases = geometry.phase_buffer();
    let mut frames = Frames::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(max_inflight);

    let start = Instant::now();
    for &target in targets {
        autd3_rs_pattern::focus(geometry, target, wavelength, &mut phases);
        frames.encode_into(geometry, write_focus(&phases))?;
        for frame in &frames {
            if pending.len() >= max_inflight {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send_frame(frame).await?);
        }
    }
    while let Some(response) = pending.pop_front() {
        response.await?.check()?;
    }
    Ok(start.elapsed())
}

async fn configure(client: &Client) -> Result<()> {
    let phases = client.geometry().phase_buffer();
    client
        .send((
            WritePatternBuffer::new(PatternBank::B0, 0, &phases, Intensity::MIN),
            ConfigPattern {
                bank: PatternBank::B0,
                config: SamplingConfig::FREQ_4K,
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            },
        ))
        .await?;
    Ok(())
}

fn write_focus(phases: &[Vec<Phase>]) -> WritePatternBuffer<'_> {
    WritePatternBuffer::new(PatternBank::B0, 0, phases, Intensity::MAX)
}

fn report(label: &str, elapsed: Duration) {
    let rate = (TOTAL_POINTS as f64) / elapsed.as_secs_f64();
    println!("{label}: {TOTAL_POINTS} updates in {elapsed:.2?} ({rate:.0} updates/s)");
}
