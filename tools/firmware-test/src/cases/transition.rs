use core::f32::consts::PI;

use anyhow::Result;

use autd3_rs::commands::{
    ChangePatternBank, EmulateGpioIn, FociStm, FociStmOption, Modulation, SetSilencer,
};
use autd3_rs::geometry::{Point3, Vector3, offset};
use autd3_rs::units::{Hz, mm};
use autd3_rs::value::{
    ControlPoint, ControlPoints, DcSysTime, GpioIn, Intensity, LoopBehavior, PatternBank, Phase,
    SamplingConfig, TransitionMode,
};
use autd3_rs_modulation::{SineOption, constant, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::pattern_util::write_foci_bank;
use crate::io::wait_enter;

const POINT_NUM: usize = 200;
const RADIUS_MM: f32 = 30.0;

fn circle_foci(center: Point3<f32>) -> Vec<ControlPoints<1>> {
    (0..POINT_NUM)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / POINT_NUM as f32;
            let p = center + Vector3::new(RADIUS_MM * theta.cos(), RADIUS_MM * theta.sin(), 0.0);
            ControlPoints::new([ControlPoint::new(p, Phase::ZERO)], Intensity::MAX)
        })
        .collect()
}

async fn change_bank(
    ctx: &Ctx<'_>,
    bank: PatternBank,
    transition_mode: TransitionMode,
) -> Result<()> {
    ctx.send(ChangePatternBank {
        bank,
        transition_mode,
    })
    .await
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let center = ctx.geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = circle_foci(center);
    let mut reversed = foci.clone();
    reversed.reverse();
    reversed[POINT_NUM - 1].intensity = Intensity::MIN;

    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &static_ff))
        .await?;

    let mut builder = ctx.client.datagram_builder();
    builder.push(SetSilencer::disable()).push(FociStm::new(
        0.5 * Hz,
        &foci,
        FociStmOption::default(),
    ));
    for frame in &builder.build()? {
        ctx.client.send_checked(frame).await?;
    }
    wait_enter("A 0.5 Hz STM runs on a 30 mm-radius circle 150 mm above the array centre").await;

    write_foci_bank(
        ctx,
        PatternBank::B1,
        0.5 * Hz,
        &reversed,
        LoopBehavior::ONCE,
    )
    .await?;
    wait_enter("Nothing changed. Press Enter when the focus reaches the device's left edge").await;
    let at = DcSysTime::from_nanos(DcSysTime::now().sys_time() + 2_000_000_000);
    change_bank(ctx, PatternBank::B1, TransitionMode::SysTime(at)).await?;
    wait_enter(
        "~2 s later the trajectory jumps to the right edge, runs in reverse, then stops after one cycle",
    )
    .await;

    change_bank(ctx, PatternBank::B0, TransitionMode::Immediate).await?;
    wait_enter("The 0.5 Hz STM is applied again").await;

    write_foci_bank(
        ctx,
        PatternBank::B1,
        0.5 * Hz,
        &reversed,
        LoopBehavior::ONCE,
    )
    .await?;
    wait_enter("Press Enter when the focus reaches the device's left edge").await;
    ctx.send(EmulateGpioIn {
        values: [true, false, false, false],
    })
    .await?;
    change_bank(ctx, PatternBank::B1, TransitionMode::Gpio(GpioIn::I0)).await?;
    wait_enter(
        "On the GPIO-in I0 trigger the trajectory jumps to the right edge, runs in reverse, then stops",
    )
    .await;
    ctx.send(EmulateGpioIn {
        values: [false, false, false, false],
    })
    .await?;

    ext_square(ctx, center).await?;
    Ok(())
}

fn vertex(center: Point3<f32>, dx: f32, dy: f32) -> ControlPoints<1> {
    ControlPoints::new(
        [ControlPoint::new(
            center + Vector3::new(dx, dy, 0.0),
            Phase::ZERO,
        )],
        Intensity::MAX,
    )
}

async fn ext_square(ctx: &Ctx<'_>, center: Point3<f32>) -> Result<()> {
    let mut sine150 = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut sine150)?;
    ctx.send(Modulation::new(SamplingConfig::FREQ_4K, &sine150))
        .await?;
    let square_a = vec![vertex(center, 30.0, 0.0), vertex(center, 0.0, 30.0)];
    let square_b = vec![vertex(center, -30.0, 0.0), vertex(center, 0.0, -30.0)];
    ctx.send(FociStm::new(0.5 * Hz, &square_a, FociStmOption::default()))
        .await?;
    ctx.send(FociStm::new(
        0.5 * Hz,
        &square_b,
        FociStmOption {
            bank: PatternBank::B1,
            transition_mode: TransitionMode::Ext,
            ..FociStmOption::default()
        },
    ))
    .await?;
    wait_enter("The focus jumps between the square's vertices every second (Ext transition)").await;
    Ok(())
}
