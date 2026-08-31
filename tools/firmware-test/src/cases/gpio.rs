use anyhow::Result;

use autd3_rs::commands::{GpioOut, SetGpioOut};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::value::{DcSysTime, Emission, Intensity, Phase, SamplingConfig};
use autd3_rs_modulation::{constant, modulation_buffer};

use crate::Ctx;
use crate::cases::pattern_util::send_pattern_mod;
use crate::io::wait_enter;

const TR_A: u8 = 0;
const TR_B: u8 = 248;

async fn send_gpio(ctx: &Ctx<'_>, outputs: [GpioOut; 4]) -> Result<()> {
    ctx.send(SetGpioOut { outputs }).await
}

async fn send_gpio_each(ctx: &Ctx<'_>, outputs: impl Fn(usize) -> [GpioOut; 4]) -> Result<()> {
    let mut builder = ctx.client.datagram_builder();
    builder.push_each(|dev| {
        Some(SetGpioOut {
            outputs: outputs(dev.idx()),
        })
    });
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }
    Ok(())
}

fn custom_drive(ctx: &Ctx<'_>) -> Vec<Vec<Emission>> {
    let mut em = ctx.geometry.pattern_buffer();
    for (d, dev) in ctx.geometry.iter().enumerate() {
        let (i0, p0, i248, p248) = if d == 0 {
            (0xFF, 0x00, 0x80, 0x80)
        } else {
            (0xFF, 0x80, 0x80, 0x00)
        };
        em[d][usize::from(TR_A)] = Emission {
            phase: Phase(p0),
            intensity: Intensity(i0),
        };
        if dev.num_transducers() > usize::from(TR_B) {
            em[d][usize::from(TR_B)] = Emission {
                phase: Phase(p248),
                intensity: Intensity(i248),
            };
        }
    }
    em
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let em = custom_drive(ctx);
    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);
    send_pattern_mod(ctx, &em, &static_ff, SamplingConfig::FREQ_4K).await?;

    send_gpio(
        ctx,
        [
            GpioOut::BaseSignal,
            GpioOut::Off,
            GpioOut::Off,
            GpioOut::Off,
        ],
    )
    .await?;
    wait_enter("GPIO[0] outputs BaseSignal and there is no output on GPIO[1]").await;

    send_gpio(
        ctx,
        [
            GpioOut::BaseSignal,
            GpioOut::PwmOut(TR_A),
            GpioOut::Off,
            GpioOut::Off,
        ],
    )
    .await?;
    wait_enter("GPIO[1] shows a 50% duty square wave, phase-shifted half a period between devices")
        .await;

    send_gpio(
        ctx,
        [
            GpioOut::BaseSignal,
            GpioOut::PwmOut(TR_B),
            GpioOut::Off,
            GpioOut::Off,
        ],
    )
    .await?;
    wait_enter(
        "GPIO[1] shows an ~17% duty square wave, phase-shifted half a period between devices",
    )
    .await;

    send_gpio_each(ctx, |dev| {
        let tr = if dev == 0 { TR_A } else { TR_B };
        [
            GpioOut::BaseSignal,
            GpioOut::PwmOut(tr),
            GpioOut::Off,
            GpioOut::Off,
        ]
    })
    .await?;
    wait_enter("The GPIO[1] square waves are now phase-aligned across devices").await;

    send_gpio_each(ctx, |dev| {
        let tr = if dev == 0 { 1 } else { 2 };
        [
            GpioOut::BaseSignal,
            GpioOut::PwmOut(tr),
            GpioOut::Off,
            GpioOut::Off,
        ]
    })
    .await?;
    wait_enter("There is no output on GPIO[1] of any device").await;

    wait_enter(
        "The next step fires a single one-shot trigger on GPIO[1] about 2 s after you continue.\n\
         Arm the oscilloscope to single/normal-trigger on a GPIO[1] rising edge before pressing Enter",
    )
    .await;

    let t0 = DcSysTime::now()? + std::time::Duration::from_secs(2);
    send_gpio_each(ctx, |dev| {
        let at = if dev == 0 { t0 } else { t0 + ULTRASOUND_PERIOD };
        [
            GpioOut::BaseSignal,
            GpioOut::SysTimeEq(at),
            GpioOut::Off,
            GpioOut::Off,
        ]
    })
    .await?;
    wait_enter(
        "~2 s later a single trigger fires on GPIO[1], offset 25 us between device 0 and the others",
    )
    .await;

    send_gpio(ctx, [GpioOut::Off; 4]).await?;
    Ok(())
}
