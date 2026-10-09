use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::Frames;
use autd3_rs::commands::{PatternStm, PatternStmOption, SetSilencer};
use autd3_rs::units::Hz;
use autd3_rs::value::{
    Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs_modulation::{constant, modulation_buffer};

use crate::Ctx;
use crate::cases::pattern_util::{
    Buffers, POINT_NUM, RADIUS_MM, activate_pattern_bank, buffers,
    expect_transition_mode_rejections, focus_at, report_fpga_state, write_pattern_stm_bank,
};
use crate::io::wait_enter;

fn circle_patterns(ctx: &Ctx<'_>) -> Vec<Buffers> {
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

type Split = (Vec<Vec<Vec<Phase>>>, Vec<Vec<Vec<Intensity>>>);

fn split(patterns: &[Buffers]) -> Split {
    patterns.iter().cloned().unzip()
}

async fn send_stm(
    ctx: &Ctx<'_>,
    patterns: &[Buffers],
    config: f32,
    bank: PatternBank,
) -> Result<()> {
    let (phases, intensities) = split(patterns);
    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (
            SetSilencer::default(),
            PatternStm::new(
                config * Hz,
                &phases,
                &intensities,
                PatternStmOption {
                    bank,
                    ..PatternStmOption::default()
                },
            ),
        ),
    )?)
    .await
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

    activate_pattern_bank(ctx, PatternBank::B0, TransitionMode::Immediate).await?;
    wait_enter("The STM frequency returned to 0.5 Hz").await;
    report_fpga_state(ctx, "back to B0", None, Some(PatternBank::B0), Some(false)).await?;

    let mut rev = patterns.clone();
    rev.reverse();
    rev[POINT_NUM - 1] = buffers(ctx.geometry, Intensity::MIN);
    write_pattern_stm_bank(ctx, PatternBank::B1, 0.5 * Hz, &rev, LoopBehavior::ONCE).await?;
    wait_enter("Nothing changed. Press Enter when the focus reaches the device's left edge").await;
    activate_pattern_bank(ctx, PatternBank::B1, TransitionMode::SyncIdx).await?;
    wait_enter("The trajectory reverses at the right edge, then stops after one cycle").await;

    let (phases, intensities) = split(&patterns);
    expect_transition_mode_rejections(ctx, "PatternSTM", |loop_behavior, transition_mode| {
        Frames::encode(
            ctx.client.geometry(),
            (
                SetSilencer::default(),
                PatternStm::new(
                    0.5 * Hz,
                    &phases,
                    &intensities,
                    PatternStmOption {
                        loop_behavior,
                        transition_mode,
                        ..PatternStmOption::default()
                    },
                ),
            ),
        )
    })
    .await;
    Ok(())
}
