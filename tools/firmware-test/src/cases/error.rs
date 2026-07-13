use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{ChangePatternBank, ConfigModulation, ConfigPattern, SetSilencer};
use autd3_rs::value::{
    DcSysTime, LoopBehavior, ModulationBank, PatternBank, SamplingConfig, TransitionMode,
};

use crate::Ctx;
use crate::cases::pattern_util::{expect_firmware_error, expect_firmware_ok};
use crate::cases::{
    ERR_INVALID_SILENCER_SETTING, ERR_INVALID_TRANSITION_MODE, ERR_MISS_TRANSITION_TIME,
};
use crate::io::wait_enter;

fn divider(steps: u16) -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(steps).unwrap())
}

async fn config_bank(ctx: &Ctx<'_>, bank: PatternBank, loop_behavior: LoopBehavior) -> Result<()> {
    ctx.send(ConfigPattern {
        bank,
        config: SamplingConfig::new(NonZeroU16::MAX),
        size: 1,
        loop_behavior,
    })
    .await
}

async fn config_mod_bank(ctx: &Ctx<'_>, config: SamplingConfig) -> Result<()> {
    ctx.send(ConfigModulation {
        bank: ModulationBank::B0,
        config,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    })
    .await
}

async fn silencer_phase_checks(ctx: &Ctx<'_>) -> Result<()> {
    const DIVIDER: u16 = 20;

    config_mod_bank(ctx, SamplingConfig::new(NonZeroU16::MAX)).await?;
    config_bank(ctx, PatternBank::B0, LoopBehavior::Infinite).await?;
    ctx.send(SetSilencer::default()).await?;

    expect_firmware_ok(
        ctx,
        "modulation divider within phase window (amplitude only)",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(ConfigModulation {
                bank: ModulationBank::B0,
                config: divider(DIVIDER),
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            });
            b.build()
        },
    )
    .await;

    expect_firmware_error(
        ctx,
        "pattern divider within phase window (config-time)",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(ConfigPattern {
                bank: PatternBank::B0,
                config: divider(DIVIDER),
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            });
            b.build()
        },
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
    expect_firmware_error(
        ctx,
        "pattern divider within phase window (silencer-enable-time)",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(SetSilencer::default());
            b.build()
        },
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    println!("firmware error-detail checks:");

    config_bank(ctx, PatternBank::B1, LoopBehavior::Infinite).await?;
    expect_firmware_error(
        ctx,
        "infinite loop + SyncIdx",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(ChangePatternBank {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::SyncIdx,
            });
            b.build()
        },
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;

    config_bank(ctx, PatternBank::B1, LoopBehavior::ONCE).await?;
    expect_firmware_error(
        ctx,
        "SysTime in the past",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(ChangePatternBank {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::SysTime {
                    time: DcSysTime::from_nanos(0),
                    margin: None,
                },
            });
            b.build()
        },
        ERR_MISS_TRANSITION_TIME,
    )
    .await;

    ctx.send(SetSilencer::default()).await?;
    expect_firmware_error(
        ctx,
        "strict silencer vs short sampling period",
        {
            let mut b = ctx.client.datagram_builder();
            b.push(ConfigPattern {
                bank: PatternBank::B0,
                config: SamplingConfig::new(NonZeroU16::new(1).unwrap()),
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            });
            b.build()
        },
        ERR_INVALID_SILENCER_SETTING,
    )
    .await;

    silencer_phase_checks(ctx).await?;

    let errors = ctx.client.read_error_detail().await?;
    for (i, code) in errors.iter().enumerate() {
        println!("  device[{i}] latched error-detail: {code:#04x}");
    }

    wait_enter("The firmware rejected each malformed command with the expected error code").await;
    Ok(())
}
