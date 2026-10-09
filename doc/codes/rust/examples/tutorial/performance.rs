use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{ActivatePatternBank, ConfigPattern, SetSilencer, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs::{Client, ClientConfig, Frames, MAX_INFLIGHT, ResponseFuture};
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

    let mut phases = geometry.phase_buffer();

    {
        // ANCHOR: configure
        client.send(SetSilencer::disable()).await?;
        client.send(WritePatternBuffer::new(PatternBank::B0, 0, &phases, Intensity::MIN)).await?;
        client.send(ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        }).await?;
        client.send(ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::Immediate,
        }).await?;
        // ANCHOR_END: configure
    }

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: hot_loop
    let mut buf = Frames::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(MAX_INFLIGHT);
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
            &mut phases,
        );

        buf.encode_into(
            &geometry,
            WritePatternBuffer::new(PatternBank::B0, 0, &phases, Intensity::MAX),
        )?;
        for frame in &buf {
            if pending.len() >= MAX_INFLIGHT {
                pending.pop_front().expect("non-empty").await?.check()?;
            }
            pending.push_back(client.send_frame(frame).await?);
        }
    }
    while let Some(fut) = pending.pop_front() {
        fut.await?.check()?;
    }
    // ANCHOR_END: hot_loop

    client.silent_stop().await?;
    client.close().await?;
    Ok(())
}
