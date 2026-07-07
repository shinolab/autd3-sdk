use core::f32::consts::PI;
use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{FixedCompletionTime, FociStm, FociStmOption, Modulation, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Vector3, offset};
use autd3_rs::mirror::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{ControlPoint, ControlPoints, Intensity, Phase, SamplingConfig};
use autd3_rs_modulation::{SineOption, constant, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::pattern_util::{focus_at, send_pattern_mod};
use crate::io::wait_enter;

fn completion_time(intensity_mul: u32, phase_mul: u32) -> FixedCompletionTime {
    FixedCompletionTime {
        intensity: ULTRASOUND_PERIOD
            * (u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY) * intensity_mul),
        phase: ULTRASOUND_PERIOD * (u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE) * phase_mul),
        strict_mode: true,
    }
}

async fn sweep_silencer(ctx: &Ctx<'_>) -> Result<()> {
    ctx.send(SetSilencer::new(completion_time(2, 2))).await?;
    wait_enter("With a longer completion time, the noise got quieter").await;

    ctx.send(SetSilencer::default()).await?;
    wait_enter("Back to default, the noise got louder").await;

    ctx.send(SetSilencer::new(FixedCompletionTime {
        intensity: ULTRASOUND_PERIOD * (u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY) / 2),
        phase: ULTRASOUND_PERIOD * (u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE) / 2),
        strict_mode: true,
    }))
    .await?;
    wait_enter("With a shorter completion time, the noise got louder").await;

    ctx.send(SetSilencer::disable()).await?;
    wait_enter("With the silencer disabled, the noise is at its loudest").await;
    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    ctx.send(SetSilencer::default()).await?;
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);
    let mod_config = SamplingConfig::new(NonZeroU16::new(20).unwrap());
    let mut sine150 = modulation_buffer();
    sine(
        150 * Hz,
        &SineOption {
            sampling_config: mod_config,
            ..Default::default()
        },
        &mut sine150,
    )?;
    send_pattern_mod(ctx, &focus, &sine150, mod_config).await?;
    wait_enter("150 Hz AM is applied").await;
    sweep_silencer(ctx).await?;

    ctx.send(SetSilencer::default()).await?;
    let center = ctx.geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci: Vec<ControlPoints<1>> = (0..10)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 10.0;
            let p = center + Vector3::new(30.0 * theta.cos(), 30.0 * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::new(p, Phase::ZERO)], Intensity::MAX)
        })
        .collect();
    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(Modulation::new(
        SamplingConfig::new(NonZeroU16::MAX),
        &static_ff,
    ))
    .await?;
    ctx.send(FociStm::new(50.0 * Hz, &foci, FociStmOption::default()))
        .await?;
    wait_enter("A 50 Hz STM is applied").await;
    sweep_silencer(ctx).await?;
    Ok(())
}
