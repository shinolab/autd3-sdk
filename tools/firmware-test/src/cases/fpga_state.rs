use core::num::NonZeroU16;
use std::time::{Duration, Instant};

use anyhow::Result;

use autd3_rs::FpgaState;
use autd3_rs::commands::{
    ActivateModulationBank, ActivatePatternBank, ConfigModulation, EmulateGpioIn, SetSilencer,
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

fn slow_sampling() -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(20_000).unwrap())
}

fn quiet_foci(center: Point3<f32>) -> Vec<ControlPoints> {
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
            state.0,
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
        if start.elapsed() > Duration::from_secs(10) {
            for (dev, state) in states.iter().enumerate() {
                if !pred(state) {
                    println!(
                        "  [FAIL] {label} device[{dev}]: expected {expected} within 10s (raw={:#04x} pattern_stopped={} mod_stopped={} transition_pending={})",
                        state.0,
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

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let center = ctx.geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);

    ctx.send(SetSilencer::disable()).await?;
    expect_state(
        ctx,
        "baseline",
        "!pattern_stopped && !mod_stopped && !transition_pending",
        |s| !s.is_pattern_stopped() && !s.is_mod_stopped() && !s.is_transition_pending(),
    )
    .await?;

    println!("pattern finite loop (SyncIdx transition):");
    write_foci_bank(
        ctx,
        PatternBank::B1,
        slow_sampling(),
        &quiet_foci(center),
        LoopBehavior::ONCE,
    )
    .await?;
    ctx.send(ActivatePatternBank {
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
    ctx.send(ActivatePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    expect_state(ctx, "back to the infinite bank", "!pattern_stopped", |s| {
        !s.is_pattern_stopped() && !s.is_transition_pending()
    })
    .await?;
    println!("pattern finite loop (GPIO transition):");
    write_foci_bank(
        ctx,
        PatternBank::B1,
        slow_sampling(),
        &quiet_foci(center),
        LoopBehavior::ONCE,
    )
    .await?;
    ctx.send(ActivatePatternBank {
        bank: PatternBank::B1,
        transition_mode: TransitionMode::Gpio(GpioIn::I0),
    })
    .await?;
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
    ctx.send(ActivatePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    println!("modulation finite loop (SyncIdx transition):");
    let data = [0xFFu8; 4];
    ctx.send(WriteModulationBuffer {
        bank: ModulationBank::B1,
        offset: 0,
        data: &data,
    })
    .await?;
    ctx.send(ConfigModulation {
        bank: ModulationBank::B1,
        config: slow_sampling(),
        size: data.len(),
        loop_behavior: LoopBehavior::ONCE,
    })
    .await?;
    ctx.send(ActivateModulationBank {
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
    ctx.send(ActivateModulationBank {
        bank: ModulationBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    expect_state(ctx, "back to the infinite bank", "!mod_stopped", |s| {
        !s.is_mod_stopped() && !s.is_transition_pending()
    })
    .await?;

    wait_enter("Every FPGA-state check above reported OK").await;
    Ok(())
}
