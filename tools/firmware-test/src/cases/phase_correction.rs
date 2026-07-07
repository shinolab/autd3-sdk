use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::SetPhaseCorrection;
use autd3_rs::geometry::offset;
use autd3_rs::units::Hz;
use autd3_rs::units::{m, mm, rad, s};
use autd3_rs::value::{Emission, Intensity, Phase, SamplingConfig};
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};
use autd3_rs_pattern::{uniform, wavelength};

use crate::Ctx;
use crate::cases::pattern_util::{SOUND_SPEED_M_S, send_pattern_mod};
use crate::io::wait_enter;

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let wavenumber = 2.0 * PI / wavelength(SOUND_SPEED_M_S * m / s).mm();

    let phases: Vec<Vec<Phase>> = ctx
        .geometry
        .iter()
        .map(|dev| {
            let target = dev.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
            (0..dev.num_transducers())
                .map(|i| {
                    let dist = (target - dev.position(i)).norm();
                    Phase::from(-(dist * wavenumber) * rad)
                })
                .collect()
        })
        .collect();
    ctx.send(SetPhaseCorrection { phases: &phases }).await?;

    let mut emissions = ctx.geometry.pattern_buffer();
    uniform(
        Emission {
            phase: Phase::ZERO,
            intensity: Intensity(0xFF),
        },
        &mut emissions,
    );
    let mut modbuf = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut modbuf)?;
    send_pattern_mod(ctx, &emissions, &modbuf, SamplingConfig::FREQ_4K).await?;
    wait_enter("Phase correction alone forms a focus 150 mm above the centre of each device").await;

    let zero: Vec<Vec<Phase>> = ctx
        .geometry
        .iter()
        .map(|dev| vec![Phase::ZERO; dev.num_transducers()])
        .collect();
    ctx.send(SetPhaseCorrection { phases: &zero }).await?;
    wait_enter("Phase correction is cleared and the focus collapses").await;
    Ok(())
}
