use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{PatternStm, PatternStmOption, SetSilencer};
use autd3_rs::units::Hz;
use autd3_rs::value::{Emission, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs_modulation::{constant, modulation_buffer};
use autd3_rs_pattern::null;

use crate::Ctx;
use crate::cases::ERR_INVALID_TRANSITION_MODE;
use crate::cases::pattern_util::{
    change_pattern_bank, change_pattern_bank_sync, expect_firmware_error, focus_at,
    report_fpga_state, write_pattern_stm_bank,
};
use crate::io::wait_enter;

const POINT_NUM: usize = 200;
const RADIUS_MM: f32 = 30.0;

fn circle_patterns(ctx: &Ctx<'_>) -> Vec<Vec<Vec<Emission>>> {
    (0..POINT_NUM)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / POINT_NUM as f32;
            focus_at(
                ctx.geometry,
                [RADIUS_MM * theta.cos(), RADIUS_MM * theta.sin(), 150.0],
                0xFF,
            )
        })
        .collect()
}

async fn send_stm(
    ctx: &Ctx<'_>,
    patterns: &[Vec<Vec<Emission>>],
    config: f32,
    bank: PatternBank,
) -> Result<()> {
    let mut builder = ctx.client.datagram_builder();
    builder.push(SetSilencer::default()).push(PatternStm::new(
        config * Hz,
        patterns,
        PatternStmOption {
            bank,
            ..PatternStmOption::default()
        },
    ));
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(autd3_rs::commands::Modulation::new(
        SamplingConfig::FREQ_4K,
        &static_ff,
    ))
    .await?;

    let patterns = circle_patterns(ctx);

    send_stm(ctx, &patterns, 0.5, PatternBank::B0).await?;
    wait_enter("A 0.5 Hz STM runs on a 30 mm-radius circle centred 150 mm above the array centre")
        .await;
    report_fpga_state(ctx, "B0 0.5Hz", None, Some(PatternBank::B0), Some(false)).await?;

    send_stm(ctx, &patterns, 1.0, PatternBank::B1).await?;
    wait_enter("The STM frequency changed to 1 Hz").await;
    report_fpga_state(ctx, "B1 1Hz", None, Some(PatternBank::B1), Some(false)).await?;

    change_pattern_bank(ctx, PatternBank::B0).await?;
    wait_enter("The STM frequency returned to 0.5 Hz").await;
    report_fpga_state(ctx, "back to B0", None, Some(PatternBank::B0), Some(false)).await?;

    let mut rev = patterns.clone();
    rev.reverse();
    let mut last = ctx.geometry.pattern_buffer();
    null(&mut last);
    rev[POINT_NUM - 1] = last;
    write_pattern_stm_bank(ctx, PatternBank::B1, 0.5 * Hz, &rev, LoopBehavior::ONCE).await?;
    wait_enter("Nothing changed. Press Enter when the focus reaches the device's left edge").await;
    change_pattern_bank_sync(ctx, PatternBank::B1).await?;
    wait_enter("The trajectory reverses at the right edge, then stops after one cycle").await;

    println!("transition-mode validation (firmware):");
    expect_firmware_error(
        ctx,
        "PatternSTM infinite loop + SyncIdx",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(SetSilencer::default()).push(PatternStm::new(
                0.5 * Hz,
                &patterns,
                PatternStmOption {
                    loop_behavior: LoopBehavior::Infinite,
                    transition_mode: TransitionMode::SyncIdx,
                    ..PatternStmOption::default()
                },
            ));
            b.build()
        },
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
    expect_firmware_error(
        ctx,
        "PatternSTM finite loop + Immediate",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(SetSilencer::default()).push(PatternStm::new(
                0.5 * Hz,
                &patterns,
                PatternStmOption {
                    loop_behavior: LoopBehavior::ONCE,
                    transition_mode: TransitionMode::Immediate,
                    ..PatternStmOption::default()
                },
            ));
            b.build()
        },
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
    Ok(())
}
