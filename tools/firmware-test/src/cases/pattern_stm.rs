use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{ActivatePatternBank, PatternStm, PatternStmOption, SetSilencer};
use autd3_rs::units::Hz;
use autd3_rs::value::{
    Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs_modulation::{constant, modulation_buffer};

use crate::Ctx;
use crate::cases::pattern_util::{
    Buffers, buffers, focus_at, report_fpga_state, stm_sampling, write_pattern_stm_bank,
};
use crate::io::wait_enter;

type Split = (Vec<Vec<Vec<Phase>>>, Vec<Vec<Vec<Intensity>>>);

async fn send_stm(
    ctx: &Ctx<'_>,
    patterns: &[Buffers],
    config: f32,
    bank: PatternBank,
) -> Result<()> {
    let (phases, intensities): Split = patterns.iter().cloned().unzip();
    ctx.send(SetSilencer::default()).await?;
    ctx.send(PatternStm::new(
        stm_sampling(config * Hz, phases.len())?,
        &phases,
        &intensities,
        PatternStmOption {
            bank,
            ..PatternStmOption::default()
        },
    ))
    .await
}

pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(autd3_rs::commands::Modulation::new(
        SamplingConfig::FREQ_4K,
        &static_ff,
    ))
    .await?;

    let patterns: Vec<Buffers> = (0..200)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 200.0;
            focus_at(
                ctx.geometry,
                [30.0 * theta.cos(), 30.0 * theta.sin(), 150.0],
                0xFF,
            )
        })
        .collect();

    send_stm(ctx, &patterns, 0.5, PatternBank::B0).await?;
    wait_enter("A 0.5 Hz STM runs on a 30 mm-radius circle centred 150 mm above the array centre")
        .await;
    report_fpga_state(ctx, "B0 0.5Hz", None, Some(PatternBank::B0), Some(false)).await?;

    send_stm(ctx, &patterns, 1.0, PatternBank::B1).await?;
    wait_enter("The STM frequency changed to 1 Hz").await;
    report_fpga_state(ctx, "B1 1Hz", None, Some(PatternBank::B1), Some(false)).await?;

    ctx.send(ActivatePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    wait_enter("The STM frequency returned to 0.5 Hz").await;
    report_fpga_state(ctx, "back to B0", None, Some(PatternBank::B0), Some(false)).await?;

    let mut rev = patterns.clone();
    rev.reverse();
    rev[199] = buffers(ctx.geometry, Intensity::MIN);
    write_pattern_stm_bank(
        ctx,
        PatternBank::B1,
        stm_sampling(0.5 * Hz, rev.len())?,
        &rev,
        LoopBehavior::ONCE,
    )
    .await?;
    wait_enter("Nothing changed. Press Enter when the focus reaches the device's left edge").await;
    ctx.send(ActivatePatternBank {
        bank: PatternBank::B1,
        transition_mode: TransitionMode::SyncIdx,
    })
    .await?;
    wait_enter("The trajectory reverses at the right edge, then stops after one cycle").await;
    Ok(())
}
