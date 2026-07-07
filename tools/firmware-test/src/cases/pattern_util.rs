use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{
    ChangeModulationBank, ChangePatternBank, ConfigFociStm, ConfigModulation, ConfigPattern,
    Modulation, Pattern, SetSilencer, StmConfig, WriteFociBuffer, WriteModulationBuffer,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{
    ControlPoints, Emission, Intensity, LoopBehavior, ModulationBank, PatternBank, Phase,
    SamplingConfig, TransitionMode,
};
use autd3_rs::{Error, Frames, Velocity};
use autd3_rs_pattern::{FocusOption, focus, wavelength};

use crate::Ctx;

pub const SOUND_SPEED_M_S: f32 = 340.0;

pub fn focus_at(geometry: &Geometry, off: [f32; 3], intensity: u8) -> Vec<Vec<Emission>> {
    let mut buf = geometry.pattern_buffer();
    let target = geometry.center() + offset(off[0] * mm, off[1] * mm, off[2] * mm);
    let wl = wavelength(SOUND_SPEED_M_S * m / s);
    focus(
        geometry,
        target,
        wl,
        &FocusOption {
            intensity: Intensity(intensity),
            phase_offset: Phase::ZERO,
        },
        &mut buf,
    );
    buf
}

pub async fn send_pattern_mod(
    ctx: &Ctx<'_>,
    emissions: &[Vec<Emission>],
    modulation: &[u8],
    config: SamplingConfig,
) -> Result<()> {
    let mut builder = ctx.client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(emissions))
        .push(Modulation::new(config, modulation));
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

pub async fn write_pattern_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    emissions: &[Vec<Emission>],
) -> Result<()> {
    let mut builder = ctx.client.datagram_builder();
    builder
        .push(WritePatternBuffer {
            bank,
            index: 0,
            emissions,
        })
        .push(ConfigPattern {
            bank,
            config: SamplingConfig::new(NonZeroU16::MAX),
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        });
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

pub async fn change_pattern_bank(ctx: &Ctx<'_>, bank: PatternBank) -> Result<()> {
    ctx.send(ChangePatternBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    })
    .await
}

pub async fn write_mod_bank(
    ctx: &Ctx<'_>,
    bank: ModulationBank,
    config: SamplingConfig,
    data: &[u8],
) -> Result<()> {
    let mut builder = ctx.client.datagram_builder();
    builder
        .push(WriteModulationBuffer {
            bank,
            offset: 0,
            data,
        })
        .push(ConfigModulation {
            bank,
            config,
            size: data.len(),
            loop_behavior: LoopBehavior::Infinite,
        });
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

pub async fn report_fpga_state(
    ctx: &Ctx<'_>,
    label: &str,
    mod_bank: Option<ModulationBank>,
    pattern_bank: Option<PatternBank>,
    pattern_mode: Option<bool>,
) -> Result<()> {
    let states = ctx.client.read_fpga_state().await?;
    for (dev, state) in states.iter().enumerate() {
        let mut fields = Vec::new();
        if let Some(expected) = mod_bank {
            fields.push(mark(
                "mod_bank",
                state.current_mod_bank() == expected,
                &format!("{:?}", state.current_mod_bank()),
            ));
        }
        if let Some(expected) = pattern_bank {
            fields.push(mark(
                "pattern_bank",
                state.current_pattern_bank() == expected,
                &format!("{:?}", state.current_pattern_bank()),
            ));
        }
        if let Some(expected) = pattern_mode {
            fields.push(mark(
                "mode",
                state.is_pattern_mode() == expected,
                if state.is_pattern_mode() {
                    "pattern"
                } else {
                    "stm"
                },
            ));
        }
        println!("  {label} device[{dev}]: {}", fields.join(" "));
    }
    Ok(())
}

fn mark(name: &str, ok: bool, actual: &str) -> String {
    let status = if ok { "OK" } else { "FAIL" };
    format!("[{status}] {name}={actual}")
}

pub fn expect_transition_rejected(label: &str, result: Result<Frames, Error>) {
    match result {
        Err(Error::TransitionConstraint { .. }) => {
            println!("  [OK] {label}: rejected (TransitionConstraint)");
        }
        Err(e) => println!("  [FAIL] {label}: unexpected error {e:?}"),
        Ok(_) => println!("  [FAIL] {label}: build unexpectedly succeeded"),
    }
}

pub async fn change_mod_bank(ctx: &Ctx<'_>, bank: ModulationBank) -> Result<()> {
    ctx.send(ChangeModulationBank {
        bank,
        transition_mode: TransitionMode::Immediate,
    })
    .await
}

pub async fn write_foci_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    config: impl Into<StmConfig>,
    points: &[ControlPoints<1>],
    loop_behavior: LoopBehavior,
) -> Result<()> {
    let size = points.len();
    let config = config.into().into_sampling_config(size);
    let mut builder = ctx.client.datagram_builder();
    builder
        .push(WriteFociBuffer {
            bank,
            index_offset: 0,
            points,
        })
        .push(ConfigFociStm {
            bank,
            config,
            size,
            num_foci: 1,
            sound_speed: Velocity::from_m_s(SOUND_SPEED_M_S),
            loop_behavior,
        });
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

pub async fn change_pattern_bank_sync(ctx: &Ctx<'_>, bank: PatternBank) -> Result<()> {
    ctx.send(ChangePatternBank {
        bank,
        transition_mode: TransitionMode::SyncIdx,
    })
    .await
}

pub async fn write_pattern_stm_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    config: impl Into<StmConfig>,
    patterns: &[Vec<Vec<Emission>>],
    loop_behavior: LoopBehavior,
) -> Result<()> {
    let size = patterns.len();
    let config = config.into().into_sampling_config(size);
    let mut builder = ctx.client.datagram_builder();
    for (index, pattern) in patterns.iter().enumerate() {
        builder.push(WritePatternBuffer {
            bank,
            index,
            emissions: pattern,
        });
    }
    builder.push(ConfigPattern {
        bank,
        config,
        size,
        loop_behavior,
    });
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}
