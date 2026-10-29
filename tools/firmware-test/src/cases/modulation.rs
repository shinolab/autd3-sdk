use core::num::NonZeroU16;
use core::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{ActivateModulationBank, Modulation};
use autd3_rs::params::MOD_BUFFER_SAMPLES;
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs_modulation::{SineOption, constant, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::pattern_util::{focus_at, report_fpga_state, send_pattern_mod, write_mod_bank};
use crate::io::wait_enter;

const MOD_PAGE_SAMPLES: usize = 2 * 16384;

pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);

    let mut sine150 = modulation_buffer();
    sine(
        150 * Hz,
        &SineOption::default(),
        SamplingConfig::FREQ_4K,
        &mut sine150,
    )?;
    send_pattern_mod(ctx, &focus, &sine150, SamplingConfig::FREQ_4K).await?;
    wait_enter("A focus is formed 150 mm above the centre of each device").await;

    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(Modulation::with_bank(
        ModulationBank::B1,
        SamplingConfig::FREQ_4K,
        &static_ff,
    ))
    .await?;
    wait_enter("No AM is applied").await;
    report_fpga_state(ctx, "B1 static", Some(ModulationBank::B1), None, None).await?;

    ctx.send(ActivateModulationBank {
        bank: ModulationBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    wait_enter("AM is applied again").await;
    report_fpga_state(ctx, "back to B0", Some(ModulationBank::B0), None, None).await?;

    let mut static_00 = modulation_buffer();
    constant(0x00, &mut static_00);
    write_mod_bank(ctx, ModulationBank::B1, SamplingConfig::FREQ_4K, &static_00).await?;
    wait_enter("AM is still applied").await;
    report_fpga_state(ctx, "B0 stays active", Some(ModulationBank::B0), None, None).await?;

    ctx.send(ActivateModulationBank {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    wait_enter("The focus disappears").await;
    report_fpga_state(ctx, "switch to B1", Some(ModulationBank::B1), None, None).await?;

    let period = Duration::from_micros(250)
        * u32::try_from(MOD_PAGE_SAMPLES).expect("MOD_PAGE_SAMPLES fits in u32");

    let mut front = vec![0u8; MOD_BUFFER_SAMPLES];
    for head in (0..MOD_BUFFER_SAMPLES).step_by(MOD_PAGE_SAMPLES) {
        front[head] = 0xFF;
    }
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &front))
        .await?;
    wait_enter(&format!("A single pop is heard once every {period:?}")).await;

    let mut back = vec![0u8; MOD_BUFFER_SAMPLES];
    for tail in (MOD_PAGE_SAMPLES - 1..MOD_BUFFER_SAMPLES).step_by(MOD_PAGE_SAMPLES) {
        back[tail] = 0xFF;
    }
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &back))
        .await?;
    wait_enter(&format!("A single pop is heard once every {period:?}")).await;

    ctx.send(ActivateModulationBank {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;

    let saw_config = SamplingConfig::new(const { NonZeroU16::new(156).unwrap() });
    let ramp: Vec<u8> = (0..=255u8).collect();
    ctx.send(Modulation {
        loop_behavior: LoopBehavior::ONCE,
        transition_mode: TransitionMode::SyncIdx,
        ..Modulation::with_bank(ModulationBank::B0, saw_config, &ramp)
    })
    .await?;
    wait_enter("A sawtooth AM is applied for exactly one waveform").await;

    let mut rev = ramp.clone();
    rev.reverse();
    ctx.send(Modulation {
        loop_behavior: LoopBehavior::ONCE,
        transition_mode: TransitionMode::SyncIdx,
        ..Modulation::with_bank(ModulationBank::B1, saw_config, &rev)
    })
    .await?;
    wait_enter("A reversed sawtooth AM is applied for exactly one waveform").await;
    Ok(())
}
