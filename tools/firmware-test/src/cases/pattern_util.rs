use core::f32::consts::PI;
use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{
    ActivateModulationBank, ActivatePatternBank, ConfigFociStm, ConfigModulation, ConfigPattern,
    Modulation, Pattern, SetSilencer, StmConfig, WriteFociBuffer, WriteModulationBuffer,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Geometry, Point3, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{
    ControlPoint, ControlPoints, Intensity, LoopBehavior, ModulationBank, PatternBank, Phase,
    SamplingConfig, TransitionMode,
};
use autd3_rs::{Error, Frames, Telemetry, Velocity};
use autd3_rs_pattern::{focus, set_intensity, wavelength};

use crate::Ctx;
use crate::cases::ERR_INVALID_TRANSITION_MODE;

pub const SOUND_SPEED_M_S: f32 = 340.0;
pub const POINT_NUM: usize = 200;
pub const RADIUS_MM: f32 = 30.0;
pub const TR_A: u8 = 0;
pub const TR_B: u8 = 248;

pub type Buffers = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);

pub fn buffers(geometry: &Geometry, intensity: Intensity) -> Buffers {
    let mut intensities = geometry.intensity_buffer();
    set_intensity(intensity, &mut intensities);
    (geometry.phase_buffer(), intensities)
}

pub fn focus_at(geometry: &Geometry, off: [f32; 3], intensity: u8) -> Buffers {
    let (mut phases, intensities) = buffers(geometry, Intensity(intensity));
    let target = geometry.center() + offset(off[0] * mm, off[1] * mm, off[2] * mm);
    let wl = wavelength(SOUND_SPEED_M_S * m / s);
    focus(geometry, target, wl, &mut phases);
    (phases, intensities)
}

pub fn circle_foci(center: Point3<f32>, n: usize) -> Vec<ControlPoints<1>> {
    (0..n)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / n as f32;
            let p = center + Vector3::new(RADIUS_MM * theta.cos(), RADIUS_MM * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::new(p, Phase::ZERO)], Intensity::MAX)
        })
        .collect()
}

pub async fn resyncs(ctx: &Ctx<'_>) -> Result<Vec<u32>> {
    Ok(ctx
        .client
        .read_telemetry()
        .await?
        .iter()
        .map(|t| t.get(Telemetry::SyncResync))
        .collect())
}

pub async fn send_pattern_mod(
    ctx: &Ctx<'_>,
    pattern: &Buffers,
    modulation: &[u8],
    config: SamplingConfig,
) -> Result<()> {
    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (
            SetSilencer::default(),
            Pattern::new(&pattern.0, &pattern.1),
            Modulation::new(config, modulation),
        ),
    )?)
    .await
}

pub async fn write_pattern_bank(ctx: &Ctx<'_>, bank: PatternBank, pattern: &Buffers) -> Result<()> {
    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (
            WritePatternBuffer::new(bank, 0, &pattern.0, &pattern.1),
            ConfigPattern {
                bank,
                config: SamplingConfig::new(NonZeroU16::MAX),
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            },
        ),
    )?)
    .await
}

pub async fn activate_pattern_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    transition_mode: TransitionMode,
) -> Result<()> {
    ctx.send(ActivatePatternBank {
        bank,
        transition_mode,
    })
    .await
}

pub async fn write_mod_bank(
    ctx: &Ctx<'_>,
    bank: ModulationBank,
    config: SamplingConfig,
    data: &[u8],
) -> Result<()> {
    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (
            WriteModulationBuffer {
                bank,
                offset: 0,
                data,
            },
            ConfigModulation {
                bank,
                config,
                size: data.len(),
                loop_behavior: LoopBehavior::Infinite,
            },
        ),
    )?)
    .await
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

async fn send_built(
    ctx: &Ctx<'_>,
    label: &str,
    built: Result<Frames, Error>,
) -> Option<Result<(), Error>> {
    match built {
        Ok(frames) => Some(ctx.try_send_frames(&frames).await),
        Err(e) => {
            println!("  [FAIL] {label}: rejected client-side before reaching firmware ({e:?})");
            None
        }
    }
}

pub async fn expect_firmware_ok(ctx: &Ctx<'_>, label: &str, built: Result<Frames, Error>) {
    match send_built(ctx, label, built).await {
        Some(Ok(())) => println!("  [OK] {label}: firmware accepted the command"),
        Some(Err(e)) => println!("  [FAIL] {label}: firmware rejected the command ({e:?})"),
        None => {}
    }
}

pub async fn expect_firmware_error(
    ctx: &Ctx<'_>,
    label: &str,
    built: Result<Frames, Error>,
    expected: u8,
) {
    let Some(result) = send_built(ctx, label, built).await else {
        return;
    };
    match result {
        Err(Error::DeviceError { code, .. }) if code == expected => {
            println!("  [OK] {label}: firmware rejected (code={code:#04x})");
        }
        Err(Error::DeviceError { device, code }) => {
            println!(
                "  [FAIL] {label}: device[{device}] returned code={code:#04x} (expected {expected:#04x})"
            );
        }
        Err(e) => println!("  [FAIL] {label}: unexpected error {e:?}"),
        Ok(()) => {
            println!("  [FAIL] {label}: firmware accepted the command (expected {expected:#04x})");
        }
    }
}

pub async fn expect_transition_mode_rejections(
    ctx: &Ctx<'_>,
    name: &str,
    build: impl Fn(LoopBehavior, TransitionMode) -> Result<Frames, Error>,
) {
    println!("transition-mode validation (firmware):");
    expect_firmware_error(
        ctx,
        &format!("{name} infinite loop + SyncIdx"),
        build(LoopBehavior::Infinite, TransitionMode::SyncIdx),
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
    expect_firmware_error(
        ctx,
        &format!("{name} finite loop + Immediate"),
        build(LoopBehavior::ONCE, TransitionMode::Immediate),
        ERR_INVALID_TRANSITION_MODE,
    )
    .await;
}

pub async fn activate_mod_bank(
    ctx: &Ctx<'_>,
    bank: ModulationBank,
    transition_mode: TransitionMode,
) -> Result<()> {
    ctx.send(ActivateModulationBank {
        bank,
        transition_mode,
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
    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (
            WriteFociBuffer {
                bank,
                index_offset: 0,
                points,
            },
            ConfigFociStm {
                bank,
                config,
                size,
                num_foci: 1,
                sound_speed: Velocity::from_m_s(SOUND_SPEED_M_S),
                loop_behavior,
            },
        ),
    )?)
    .await
}

pub async fn write_pattern_stm_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    config: impl Into<StmConfig>,
    patterns: &[Buffers],
    loop_behavior: LoopBehavior,
) -> Result<()> {
    let size = patterns.len();
    let config = config.into().into_sampling_config(size);
    for (index, (phases, intensities)) in patterns.iter().enumerate() {
        ctx.send(WritePatternBuffer::new(bank, index, phases, intensities))
            .await?;
    }
    ctx.send(ConfigPattern {
        bank,
        config,
        size,
        loop_behavior,
    })
    .await
}
