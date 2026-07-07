use anyhow::Result;

use autd3_rs::commands::{GpioOut, PWE_TABLE_SIZE, SetGpioOut, SetPulseWidthTable};
use autd3_rs::value::{Emission, Intensity, Phase, PulseWidth, SamplingConfig};
use autd3_rs_modulation::{constant, modulation_buffer};
use autd3_rs_pattern::uniform;

use crate::Ctx;
use crate::cases::pattern_util::send_pattern_mod;
use crate::io::wait_enter;

const TR_A: u8 = 0;
const TR_B: u8 = 248;

fn drive_pair(ctx: &Ctx<'_>, dev0: (u8, u8), other: (u8, u8)) -> Vec<Vec<Emission>> {
    let mut em = ctx.geometry.pattern_buffer();
    for (d, dev) in ctx.geometry.iter().enumerate() {
        let (a, b) = if d == 0 { dev0 } else { other };
        em[d][usize::from(TR_A)] = Emission {
            phase: Phase::ZERO,
            intensity: Intensity(a),
        };
        if dev.num_transducers() > usize::from(TR_B) {
            em[d][usize::from(TR_B)] = Emission {
                phase: Phase::ZERO,
                intensity: Intensity(b),
            };
        }
    }
    em
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    ctx.send(SetGpioOut {
        outputs: [
            GpioOut::PwmOut(TR_A),
            GpioOut::PwmOut(TR_B),
            GpioOut::Off,
            GpioOut::Off,
        ],
    })
    .await?;

    let mut static_ff = modulation_buffer();
    constant(0xFF, &mut static_ff);

    let mut table = [PulseWidth::from_duty(0.5); PWE_TABLE_SIZE];
    table[0] = PulseWidth::from_duty(6.25 / 100.0);
    table[1] = PulseWidth::from_duty(12.5 / 100.0);
    table[2] = PulseWidth::from_duty(18.75 / 100.0);
    table[3] = PulseWidth::from_duty(25.0 / 100.0);
    ctx.send(SetPulseWidthTable { table: &table }).await?;

    let em = drive_pair(ctx, (0, 1), (2, 3));
    send_pattern_mod(ctx, &em, &static_ff, SamplingConfig::FREQ_4K).await?;
    wait_enter(
        "Device 0 GPIO[0]/GPIO[1] and device 1 GPIO[0]/GPIO[1] show duty cycles of \
         6.25%, 12.5%, 18.75%, 25% respectively",
    )
    .await;

    ctx.send(SetPulseWidthTable {
        table: &[PulseWidth::new(0); PWE_TABLE_SIZE],
    })
    .await?;
    let mut full = ctx.geometry.pattern_buffer();
    uniform(
        Emission {
            phase: Phase::ZERO,
            intensity: Intensity::MAX,
        },
        &mut full,
    );
    send_pattern_mod(ctx, &full, &static_ff, SamplingConfig::FREQ_4K).await?;
    wait_enter("No output is present on GPIO[0] or GPIO[1] of any device").await;

    ctx.send(SetPulseWidthTable {
        table: &SetPulseWidthTable::default_table(),
    })
    .await?;
    let em = drive_pair(ctx, (0, 0xFF), (0, 0xFF));
    send_pattern_mod(ctx, &em, &static_ff, SamplingConfig::FREQ_4K).await?;
    wait_enter("GPIO[0] and GPIO[1] show duty cycles of 0% and 50% respectively").await;
    Ok(())
}
