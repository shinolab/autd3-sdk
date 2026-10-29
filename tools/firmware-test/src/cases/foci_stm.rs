use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{ActivatePatternBank, FociStm, FociStmOption, SetSilencer};
use autd3_rs::geometry::{Vector3, offset};
use autd3_rs::params::MAX_FOCI_TOTAL;
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{
    ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig,
    TransitionMode,
};
use autd3_rs_modulation::{SineOption, constant, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::pattern_util::{circle_foci, report_fpga_state, stm_sampling, write_foci_bank};
use crate::io::wait_enter;

async fn send_foci(
    ctx: &Ctx<'_>,
    points: &[ControlPoints],
    config: f32,
    bank: PatternBank,
) -> Result<()> {
    ctx.send(SetSilencer::disable()).await?;
    ctx.send(FociStm::new(
        stm_sampling(config * Hz, points.len())?,
        points,
        FociStmOption {
            bank,
            ..FociStmOption::default()
        },
    ))
    .await
}

pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let center = ctx.geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = circle_foci(center, 200);

    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(autd3_rs::commands::Modulation::new(
        SamplingConfig::FREQ_4K,
        &static_ff,
    ))
    .await?;

    send_foci(ctx, &foci, 0.5, PatternBank::B0).await?;
    wait_enter("A 0.5 Hz STM runs on a 30 mm-radius circle centred 150 mm above the array centre")
        .await;
    report_fpga_state(ctx, "B0 0.5Hz", None, Some(PatternBank::B0), Some(false)).await?;

    send_foci(ctx, &foci, 1.0, PatternBank::B1).await?;
    wait_enter("The STM frequency changed to 1 Hz").await;
    report_fpga_state(ctx, "B1 1Hz", None, Some(PatternBank::B1), Some(false)).await?;

    ctx.send(ActivatePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    wait_enter("The STM frequency returned to 0.5 Hz").await;
    report_fpga_state(ctx, "back to B0", None, Some(PatternBank::B0), Some(false)).await?;

    let mut rev = foci.clone();
    rev.reverse();
    rev[199].intensity = Intensity::MIN;
    write_foci_bank(
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

    let indices = MAX_FOCI_TOTAL / 8;
    let foci8: Vec<ControlPoints> = (0..indices)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / indices as f32;
            let d = Vector3::new(30.0 * theta.cos(), 30.0 * theta.sin(), 0.0);
            let plus = ControlPoint::new(center + d, Phase::ZERO);
            let minus = ControlPoint::new(center - d, Phase::ZERO);
            ControlPoints::new(
                [plus, minus, plus, minus, plus, minus, plus, minus],
                Intensity::MAX,
            )
        })
        .collect();
    let mut sine150 = modulation_buffer();
    sine(
        150 * Hz,
        &SineOption::default(),
        SamplingConfig::FREQ_4K,
        &mut sine150,
    )?;
    ctx.send(autd3_rs::commands::Modulation::new(
        SamplingConfig::FREQ_4K,
        &sine150,
    ))
    .await?;
    ctx.send(FociStm::new(
        SamplingConfig::FREQ_4K,
        &foci8,
        FociStmOption::default(),
    ))
    .await?;
    wait_enter(&format!(
        "A two-focus {:.3} Hz STM runs on the 30 mm-radius circle",
        4_000.0 / indices as f32
    ))
    .await;

    let big = circle_foci(center, MAX_FOCI_TOTAL);
    ctx.send(autd3_rs::commands::Modulation::new(
        SamplingConfig::FREQ_4K,
        &static_ff,
    ))
    .await?;
    ctx.send(FociStm::new(
        SamplingConfig::FREQ_40K,
        &big,
        FociStmOption {
            bank: PatternBank::B1,
            ..FociStmOption::default()
        },
    ))
    .await?;
    wait_enter(&format!(
        "A {:.3} Hz STM runs (maximum buffer at 40 kHz sampling)",
        40_000.0 / MAX_FOCI_TOTAL as f32
    ))
    .await;
    Ok(())
}
