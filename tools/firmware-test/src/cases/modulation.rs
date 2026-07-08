use core::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{Modulation, SetSilencer};
use autd3_rs::params::MOD_BUFFER_SAMPLES;
use autd3_rs::units::Hz;
use autd3_rs::value::{LoopBehavior, ModulationBank, Nearest, SamplingConfig, TransitionMode};
use autd3_rs_modulation::{SineOption, constant, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::ERR_INVALID_TRANSITION_MODE;
use crate::cases::pattern_util::{
    change_mod_bank, expect_firmware_error, focus_at, report_fpga_state, send_pattern_mod,
    write_mod_bank,
};
use crate::io::wait_enter;

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);

    let mut sine150 = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut sine150)?;
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

    change_mod_bank(ctx, ModulationBank::B0).await?;
    wait_enter("AM is applied again").await;
    report_fpga_state(ctx, "back to B0", Some(ModulationBank::B0), None, None).await?;

    let mut static_00 = modulation_buffer();
    constant(0x00, &mut static_00);
    write_mod_bank(ctx, ModulationBank::B1, SamplingConfig::FREQ_4K, &static_00).await?;
    wait_enter("AM is still applied").await;
    report_fpga_state(ctx, "B0 stays active", Some(ModulationBank::B0), None, None).await?;

    change_mod_bank(ctx, ModulationBank::B1).await?;
    wait_enter("No AM is applied").await;
    report_fpga_state(ctx, "switch to B1", Some(ModulationBank::B1), None, None).await?;

    let half = MOD_BUFFER_SAMPLES / 2;
    let samples = u32::try_from(MOD_BUFFER_SAMPLES).expect("MOD_BUFFER_SAMPLES fits in u32");
    let period = Duration::from_micros(250) * samples / 2;

    let mut front = Vec::with_capacity(MOD_BUFFER_SAMPLES);
    for _ in 0..2 {
        front.push(0xFF);
        front.extend(std::iter::repeat_n(0u8, half - 1));
    }
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &front))
        .await?;
    wait_enter(&format!("A single pop is heard once every {period:?}")).await;

    let mut back = Vec::with_capacity(MOD_BUFFER_SAMPLES);
    for _ in 0..2 {
        back.extend(std::iter::repeat_n(0u8, half - 1));
        back.push(0xFF);
    }
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &back))
        .await?;
    wait_enter(&format!("A single pop is heard once every {period:?}")).await;

    change_mod_bank(ctx, ModulationBank::B1).await?;

    let saw_config = SamplingConfig::new(Nearest(256.0 * Hz));
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

    println!("transition-mode validation (firmware):");
    let mut probe = modulation_buffer();
    constant(0xFF, &mut probe);
    expect_firmware_error(
        ctx,
        "modulation infinite loop + SyncIdx",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(SetSilencer::default()).push(Modulation {
                loop_behavior: LoopBehavior::Infinite,
                transition_mode: TransitionMode::SyncIdx,
                ..Modulation::with_bank(ModulationBank::B1, SamplingConfig::FREQ_4K, &probe)
            });
            b.build()
        },
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
    expect_firmware_error(
        ctx,
        "modulation finite loop + Immediate",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(SetSilencer::default()).push(Modulation {
                loop_behavior: LoopBehavior::ONCE,
                transition_mode: TransitionMode::Immediate,
                ..Modulation::with_bank(ModulationBank::B1, SamplingConfig::FREQ_4K, &probe)
            });
            b.build()
        },
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
    Ok(())
}
