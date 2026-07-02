use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{ConfigPattern, SetSilencer, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{LoopBehavior, PatternBank, SamplingConfig};
use autd3_rs::{Client, ClientConfig, Frames, MAX_IN_FLIGHT, ResponseFuture};
use autd3_rs_link_nop::Nop;

const NUM_POINTS: usize = 1000;
const RADIUS_MM: f32 = 30.0;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let mut patterns = geometry.pattern_buffer();

    {
        // ANCHOR: configure
        let mut builder = client.datagram_builder();
        builder
            .push(SetSilencer::disable())
            .push(WritePatternBuffer {
                bank: PatternBank::B0,
                index: 0,
                emissions: &patterns,
            })
            .push(ConfigPattern {
                bank: PatternBank::B0,
                config: SamplingConfig::FREQ_40K,
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            });
        for frame in &builder.build()? {
            client.send_checked(frame).await?;
        }
        // ANCHOR_END: configure
    }

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: hot_loop
    let mut buf = Frames::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(MAX_IN_FLIGHT);
    for i in 0..NUM_POINTS {
        let theta = 2.0 * PI * i as f32 / NUM_POINTS as f32;
        let target = center
            + offset(
                RADIUS_MM * theta.cos() * mm,
                RADIUS_MM * theta.sin() * mm,
                0.0 * mm,
            );
        autd3_rs_pattern::focus(
            &geometry,
            target,
            wavelength,
            &autd3_rs_pattern::FocusOption::default(),
            &mut patterns,
        );

        let mut builder = client.datagram_builder();
        builder.push(WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &patterns,
        });
        builder.build_into(&mut buf)?;
        for frame in &buf {
            if pending.len() >= MAX_IN_FLIGHT {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send(frame).await?);
        }
    }
    while let Some(fut) = pending.pop_front() {
        fut.await?.check()?;
    }
    // ANCHOR_END: hot_loop

    client.stop().await?;
    client.close().await?;
    Ok(())
}
