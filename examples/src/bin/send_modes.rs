// Sweeps a focus around a circle, sending the same update sequence two ways to
// show how the send loop chooses its mode. Run with: cargo xtask example send_modes

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, Point3, offset};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, LoopBehavior, PatternBank, SamplingConfig};
use autd3_rs::{
    Client, ClientConfig, ConfigPattern, Frames, Length, MAX_IN_FLIGHT, ResponseFuture,
    WritePatternBuffer,
};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

const TOTAL_POINTS: usize = 1000;

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

    configure(&client, &geometry).await?;

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

    let elapsed = run_stop_and_wait(&client, &geometry, &targets, wavelength).await?;
    report("stop-and-wait", elapsed);

    let elapsed = run_streaming(&client, &geometry, &targets, wavelength, MAX_IN_FLIGHT).await?;
    report("streaming", elapsed);

    client.stop().await?;
    client.close().await?;

    Ok(())
}

// One round-trip per frame: confirm each update lands before issuing the next.
async fn run_stop_and_wait(
    client: &Client,
    geometry: &Geometry,
    targets: &[Point3<f32>],
    wavelength: Length,
) -> Result<Duration> {
    let mut emissions = geometry.pattern_buffer();
    let mut buf = Frames::default();

    let start = Instant::now();
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &autd3_rs_pattern::FocusOption::default(),
            &mut emissions,
        );
        write_focus(client, &emissions, &mut buf)?;
        for frame in &buf {
            client.send_checked(frame).await?;
        }
    }
    Ok(start.elapsed())
}

// Keep `max_inflight` frames on the wire; drain responses behind the send cursor.
async fn run_streaming(
    client: &Client,
    geometry: &Geometry,
    targets: &[Point3<f32>],
    wavelength: Length,
    max_inflight: usize,
) -> Result<Duration> {
    let mut emissions = geometry.pattern_buffer();
    let mut buf = Frames::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(max_inflight);

    let start = Instant::now();
    for &target in targets {
        autd3_rs_pattern::focus(
            geometry,
            target,
            wavelength,
            &autd3_rs_pattern::FocusOption::default(),
            &mut emissions,
        );
        write_focus(client, &emissions, &mut buf)?;
        for frame in &buf {
            if pending.len() >= max_inflight {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send(frame).await?);
        }
    }
    while let Some(fut) = pending.pop_front() {
        fut.await?.check()?;
    }
    Ok(start.elapsed())
}

async fn configure(client: &Client, geometry: &Geometry) -> Result<()> {
    let mut emissions = geometry.pattern_buffer();
    autd3_rs_pattern::null(&mut emissions);
    let mut builder = client.datagram_builder();
    builder
        .push(WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &emissions,
        })
        .push(ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::FREQ_4K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        });
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }
    Ok(())
}

fn write_focus(
    client: &Client,
    emissions: &[[Emission; NUM_TRANSDUCERS]],
    buf: &mut Frames,
) -> Result<()> {
    let mut builder = client.datagram_builder();
    builder.push(WritePatternBuffer {
        bank: PatternBank::B0,
        index: 0,
        emissions,
    });
    builder.build_into(buf)?;
    Ok(())
}

fn report(label: &str, elapsed: Duration) {
    let rate = (TOTAL_POINTS as f64) / elapsed.as_secs_f64();
    println!("{label}: {TOTAL_POINTS} updates in {elapsed:.2?} ({rate:.0} updates/s)");
}
