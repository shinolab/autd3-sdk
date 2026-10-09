use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::Frames;
use autd3_rs::commands::{
    ActivateModulationBank, ActivatePatternBank, Clear, ConfigModulation, ConfigPattern,
    FixedCompletionTime, SetSilencer,
};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::value::{
    LoopBehavior, ModulationBank, PatternBank, SamplingConfig, SysTime, TransitionMode,
};

use crate::Ctx;
use crate::cases::pattern_util::{activate_mod_bank, expect_firmware_error, expect_firmware_ok};
use crate::cases::{
    ERR_INVALID_SILENCER_SETTING, ERR_INVALID_TRANSITION_MODE, ERR_MISS_TRANSITION_TIME,
};
use crate::io::wait_enter;

fn divider(steps: u16) -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(steps).unwrap())
}

async fn config_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    size: usize,
    loop_behavior: LoopBehavior,
) -> Result<()> {
    ctx.send(ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size,
        loop_behavior,
    })
    .await
}

async fn config_mod_bank(ctx: &Ctx<'_>, config: SamplingConfig) -> Result<()> {
    ctx.send(ConfigModulation {
        bank: ModulationBank::B0,
        config,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    })
    .await
}

async fn silencer_phase_checks(ctx: &Ctx<'_>) -> Result<()> {
    const DIVIDER: u16 = 20;

    config_mod_bank(ctx, SamplingConfig::new(NonZeroU16::MAX)).await?;
    config_bank(ctx, PatternBank::B0, 1, LoopBehavior::Infinite).await?;
    ctx.send(SetSilencer::default()).await?;

    expect_firmware_ok(
        ctx,
        "modulation divider within phase window (amplitude only)",
        Frames::encode(
            ctx.client.geometry(),
            ConfigModulation {
                bank: ModulationBank::B0,
                config: divider(DIVIDER),
                size: 2,
                loop_behavior: LoopBehavior::Infinite,
            },
        ),
    )
    .await;

    expect_firmware_error(
        ctx,
        "pattern divider within phase window (activation-time)",
        Frames::encode(
            ctx.client.geometry(),
            (
                ConfigPattern {
                    bank: PatternBank::B0,
                    config: divider(DIVIDER),
                    size: 1,
                    loop_behavior: LoopBehavior::Infinite,
                },
                ActivatePatternBank {
                    bank: PatternBank::B0,
                    transition_mode: TransitionMode::Immediate,
                },
            ),
        ),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    ctx.send(SetSilencer::disable()).await?;
    config_mod_bank(ctx, SamplingConfig::new(NonZeroU16::MAX)).await?;
    ctx.send(ConfigPattern {
        bank: PatternBank::B0,
        config: divider(DIVIDER),
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    })
    .await?;
    ctx.send(ActivatePatternBank {
        bank: PatternBank::B0,
        transition_mode: TransitionMode::Immediate,
    })
    .await?;
    expect_firmware_error(
        ctx,
        "pattern divider within phase window (silencer-enable-time)",
        Frames::encode(ctx.client.geometry(), SetSilencer::default()),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    Ok(())
}

fn strict_silencer(intensity_steps: u32) -> SetSilencer {
    SetSilencer::new(FixedCompletionTime {
        intensity: ULTRASOUND_PERIOD * intensity_steps,
        phase: ULTRASOUND_PERIOD * intensity_steps,
        strict_mode: true,
    })
}

async fn mod_banks(
    ctx: &Ctx<'_>,
    dividers: [u16; 2],
    b1_loop: LoopBehavior,
    activate: (ModulationBank, TransitionMode),
) -> Result<()> {
    for (bank, steps, loop_behavior) in [
        (ModulationBank::B0, dividers[0], LoopBehavior::Infinite),
        (ModulationBank::B1, dividers[1], b1_loop),
    ] {
        ctx.send(ConfigModulation {
            bank,
            config: divider(steps),
            size: 2,
            loop_behavior,
        })
        .await?;
    }
    activate_mod_bank(ctx, activate.0, activate.1).await
}

async fn strict_guard_follows_the_banks_in_use(ctx: &Ctx<'_>) -> Result<()> {
    const STEPS: u32 = 8;

    ctx.send(Clear).await?;
    mod_banks(
        ctx,
        [5, 100],
        LoopBehavior::ONCE,
        (ModulationBank::B0, TransitionMode::Immediate),
    )
    .await?;
    let at = ctx.client.device_time_now()? + std::time::Duration::from_secs(5);
    activate_mod_bank(
        ctx,
        ModulationBank::B1,
        TransitionMode::SysTime { time: at },
    )
    .await?;
    expect_firmware_error(
        ctx,
        "strict silencer ahead of a pending SysTime transition from a faster bank",
        Frames::encode(ctx.client.geometry(), strict_silencer(STEPS)),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    ctx.send(Clear).await?;
    mod_banks(
        ctx,
        [100, 5],
        LoopBehavior::Infinite,
        (ModulationBank::B0, TransitionMode::Ext),
    )
    .await?;
    expect_firmware_error(
        ctx,
        "strict silencer while Ext alternates onto a faster bank",
        Frames::encode(ctx.client.geometry(), strict_silencer(STEPS)),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    ctx.send(Clear).await?;
    mod_banks(
        ctx,
        [100, 5],
        LoopBehavior::Infinite,
        (ModulationBank::B0, TransitionMode::Immediate),
    )
    .await?;
    expect_firmware_ok(
        ctx,
        "strict silencer with a faster unused bank",
        Frames::encode(ctx.client.geometry(), strict_silencer(STEPS)),
    )
    .await;
    expect_firmware_error(
        ctx,
        "switching onto the faster bank under strict silencer",
        Frames::encode(
            ctx.client.geometry(),
            ActivateModulationBank {
                bank: ModulationBank::B1,
                transition_mode: TransitionMode::Immediate,
            },
        ),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;
    expect_firmware_ok(
        ctx,
        "re-activating the playing bank under strict silencer",
        Frames::encode(
            ctx.client.geometry(),
            ActivateModulationBank {
                bank: ModulationBank::B0,
                transition_mode: TransitionMode::Immediate,
            },
        ),
    )
    .await;

    ctx.send(Clear).await
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    println!("firmware error-detail checks:");

    config_bank(ctx, PatternBank::B1, 1, LoopBehavior::Infinite).await?;
    expect_firmware_error(
        ctx,
        "infinite loop + SyncIdx",
        Frames::encode(
            ctx.client.geometry(),
            ActivatePatternBank {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::SyncIdx,
            },
        ),
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;

    config_bank(ctx, PatternBank::B1, 2, LoopBehavior::ONCE).await?;
    expect_firmware_error(
        ctx,
        "SysTime in the past",
        Frames::encode(
            ctx.client.geometry(),
            ActivatePatternBank {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::SysTime {
                    time: SysTime::from_nanos(0),
                },
            },
        ),
        ERR_MISS_TRANSITION_TIME,
    )
    .await;

    ctx.send(SetSilencer::default()).await?;
    expect_firmware_error(
        ctx,
        "strict silencer vs short sampling period",
        Frames::encode(
            ctx.client.geometry(),
            (
                ConfigPattern {
                    bank: PatternBank::B0,
                    config: SamplingConfig::new(NonZeroU16::new(1).unwrap()),
                    size: 1,
                    loop_behavior: LoopBehavior::Infinite,
                },
                ActivatePatternBank {
                    bank: PatternBank::B0,
                    transition_mode: TransitionMode::Immediate,
                },
            ),
        ),
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    silencer_phase_checks(ctx).await?;
    strict_guard_follows_the_banks_in_use(ctx).await?;

    wait_enter("The firmware rejected each malformed command with the expected error code").await;
    Ok(())
}
