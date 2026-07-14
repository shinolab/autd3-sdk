use core::num::NonZeroU16;
use std::time::{Duration, Instant};

use anyhow::Result;

use autd3_rs::FpgaState;
use autd3_rs::commands::{
    ChangeModulationBank, ChangePatternBank, ConfigModulation, EmulateGpioIn, SetSilencer,
    WriteModulationBuffer,
};
use autd3_rs::geometry::{Point3, Vector3, offset};
use autd3_rs::units::mm;
use autd3_rs::value::{
    ControlPoint, ControlPoints, GpioIn, Intensity, LoopBehavior, ModulationBank, PatternBank,
    Phase, SamplingConfig, TransitionMode,
};

use crate::Ctx;
use crate::cases::pattern_util::write_foci_bank;
use crate::io::wait_enter;

// 2 Hz sampling x 4 points: the requested bank's index wraps every 2 s, so
// transition-pending stays observable for up to 2 s and a single finite loop
// completes 2 s after the transition fires.
const SAMPLING_DIVIDER: u16 = 20_000;
const POLL_TIMEOUT: Duration = Duration::from_secs(10);

fn slow_sampling() -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(SAMPLING_DIVIDER).unwrap())
}

fn quiet_foci(center: Point3<f32>) -> Vec<ControlPoints<1>> {
    (0..4)
        .map(|i| {
            let p = center + Vector3::new(i as f32, 0.0, 0.0);
            ControlPoints::new([ControlPoint::new(p, Phase::ZERO)], Intensity::MIN)
        })
        .collect()
}

async fn expect_state(
    ctx: &Ctx<'_>,
    label: &str,
    expected: &str,
    pred: impl Fn(&FpgaState) -> bool,
) -> Result<()> {
    let states = ctx.client.read_fpga_state().await?;
    for (dev, state) in states.iter().enumerate() {
        let mark = if pred(state) { "OK" } else { "FAIL" };
        println!(
            "  [{mark}] {label} device[{dev}]: expected {expected} (raw={:#04x} pattern_stopped={} mod_stopped={} transition_pending={})",
            state.raw(),
            state.is_pattern_stopped(),
            state.is_mod_stopped(),
            state.is_transition_pending(),
        );
    }
    Ok(())
}

async fn poll_state(
    ctx: &Ctx<'_>,
    label: &str,
    expected: &str,
    pred: impl Fn(&FpgaState) -> bool,
) -> Result<()> {
    let start = Instant::now();
    loop {
        let states = ctx.client.read_fpga_state().await?;
        if states.iter().all(&pred) {
            println!(
                "  [OK] {label}: all devices reached the expected state ({expected}) after {:.1} s",
                start.elapsed().as_secs_f32()
            );
            return Ok(());
        }
        if start.elapsed() > POLL_TIMEOUT {
            for (dev, state) in states.iter().enumerate() {
                if !pred(state) {
                    println!(
                        "  [FAIL] {label} device[{dev}]: expected {expected} within {POLL_TIMEOUT:?} (raw={:#04x} pattern_stopped={} mod_stopped={} transition_pending={})",
                        state.raw(),
                        state.is_pattern_stopped(),
                        state.is_mod_stopped(),
                        state.is_transition_pending(),
                    );
                }
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn pattern_finite_loop(ctx: &Ctx<'_>, center: Point3<f32>) -> Result<()> {
    println!("pattern finite loop (SyncIdx transition):");
    write_foci_bank(
        ctx,
        PatternBank::B1,
        slow_sampling(),
        &quiet_foci(center),
        LoopBehavior::ONCE,
    )
    .await?;
    ctx.send(ChangePatternBank {
        bank: PatternBank::B1,
        transition_mode: TransitionMode::SyncIdx,
    })
    .await?;
    expect_state(
        ctx,
        "right after the bank change",
        "transition_pending",
        |s| s.is_transition_pending() && !s.is_pattern_stopped(),
    )
    .await?;
    poll_state(
        ctx,
        "finite pattern loop",
        "pattern_stopped && !transition_pending",
        |s| s.is_pattern_stopped() && !s.is_transition_pending(),
    )
    .await?;

    ctx.send(ChangePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    expect_state(ctx, "back to the infinite bank", "!pattern_stopped", |s| {
        !s.is_pattern_stopped() && !s.is_transition_pending()
    })
    .await?;
    Ok(())
}

async fn pattern_gpio_pending(ctx: &Ctx<'_>, center: Point3<f32>) -> Result<()> {
    println!("pattern finite loop (GPIO transition):");
    write_foci_bank(
        ctx,
        PatternBank::B1,
        slow_sampling(),
        &quiet_foci(center),
        LoopBehavior::ONCE,
    )
    .await?;
    ctx.send(ChangePatternBank {
        bank: PatternBank::B1,
        transition_mode: TransitionMode::Gpio(GpioIn::I0),
    })
    .await?;
    // without the GPIO trigger the transition stays pending indefinitely
    tokio::time::sleep(Duration::from_secs(1)).await;
    expect_state(
        ctx,
        "while waiting for the GPIO trigger",
        "transition_pending",
        |s| s.is_transition_pending() && !s.is_pattern_stopped(),
    )
    .await?;

    ctx.send(EmulateGpioIn {
        values: [true, false, false, false],
    })
    .await?;
    poll_state(
        ctx,
        "finite pattern loop after the GPIO trigger",
        "pattern_stopped && !transition_pending",
        |s| s.is_pattern_stopped() && !s.is_transition_pending(),
    )
    .await?;

    ctx.send(EmulateGpioIn {
        values: [false, false, false, false],
    })
    .await?;
    ctx.send(ChangePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    Ok(())
}

async fn modulation_finite_loop(ctx: &Ctx<'_>) -> Result<()> {
    println!("modulation finite loop (SyncIdx transition):");
    let data = [0xFFu8; 4];
    let mut builder = ctx.client.datagram_builder();
    builder
        .push(WriteModulationBuffer {
            bank: ModulationBank::B1,
            offset: 0,
            data: &data,
        })
        .push(ConfigModulation {
            bank: ModulationBank::B1,
            config: slow_sampling(),
            size: data.len(),
            loop_behavior: LoopBehavior::ONCE,
        });
    for frame in &builder.build()? {
        ctx.client.send_checked(frame).await?;
    }
    ctx.send(ChangeModulationBank {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::SyncIdx,
    })
    .await?;
    expect_state(
        ctx,
        "right after the bank change",
        "transition_pending",
        |s| s.is_transition_pending() && !s.is_mod_stopped(),
    )
    .await?;
    poll_state(
        ctx,
        "finite modulation loop",
        "mod_stopped && !transition_pending",
        |s| s.is_mod_stopped() && !s.is_transition_pending(),
    )
    .await?;

    ctx.send(ChangeModulationBank {
        bank: ModulationBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    expect_state(ctx, "back to the infinite bank", "!mod_stopped", |s| {
        !s.is_mod_stopped() && !s.is_transition_pending()
    })
    .await?;
    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let center = ctx.geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);

    ctx.send(SetSilencer::disable()).await?;
    expect_state(
        ctx,
        "baseline",
        "!pattern_stopped && !mod_stopped && !transition_pending",
        |s| !s.is_pattern_stopped() && !s.is_mod_stopped() && !s.is_transition_pending(),
    )
    .await?;

    pattern_finite_loop(ctx, center).await?;
    pattern_gpio_pending(ctx, center).await?;
    modulation_finite_loop(ctx).await?;

    wait_enter("Every FPGA-state check above reported OK").await;
    Ok(())
}
