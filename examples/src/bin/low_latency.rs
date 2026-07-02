// Measures one-shot (stop-and-wait) command latency with the slave in
// low-latency mode. Run with: cargo xtask example low_latency

use std::time::{Duration, Instant};

use anyhow::Result;

use autd3_rs::commands::Pattern;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

const ITERATIONS: usize = 1000;
const WARMUP: usize = 10;
const ENABLE_LOW_LATENCY: bool = true;

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
        ClientConfig {
            low_latency: ENABLE_LOW_LATENCY,
            ..ClientConfig::default()
        },
    )
    .await?;

    println!("devices: {}", client.num_devices());

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut emissions = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption {
            intensity: Intensity::MIN,
            ..Default::default()
        },
        &mut emissions,
    );

    let mut builder = client.datagram_builder();
    builder.push(Pattern::new(&emissions));
    let datagrams = builder.build()?;

    for _ in 0..WARMUP {
        client
            .send_checked(datagrams.frame(0).expect("at least one frame"))
            .await?;
    }

    let mut latencies = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let frame = datagrams.frame(0).expect("at least one frame");
        let t = Instant::now();
        client.send_checked(frame).await?;
        latencies.push(t.elapsed());
    }

    latencies.sort_unstable();
    let sum: Duration = latencies.iter().sum();
    let avg = sum / u32::try_from(ITERATIONS).expect("iteration count fits u32");
    let min = latencies[0];
    let p50 = latencies[ITERATIONS / 2];
    let p99 = latencies[ITERATIONS * 99 / 100];
    let max = *latencies.last().expect("non-empty");

    println!("one-shot latency over {ITERATIONS} sends (low_latency={ENABLE_LOW_LATENCY}):");
    println!("  min={min:?}  p50={p50:?}  avg={avg:?}  p99={p99:?}  max={max:?}");

    client.close().await?;
    Ok(())
}
