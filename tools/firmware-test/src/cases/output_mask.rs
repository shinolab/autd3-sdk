use anyhow::Result;

use autd3_rs::commands::SetOutputMask;
use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::pattern_util::{focus_at, send_pattern_mod};
use crate::io::wait_enter;

fn half_mask(ctx: &Ctx<'_>, left_dev0: bool) -> Vec<Vec<bool>> {
    ctx.geometry
        .iter()
        .enumerate()
        .map(|(d, dev)| {
            let cx = dev.center().x;
            let want_left = if d == 0 { left_dev0 } else { !left_dev0 };
            (0..dev.num_transducers())
                .map(|i| {
                    let x = dev.position(i).x;
                    if want_left { x < cx } else { x >= cx }
                })
                .collect()
        })
        .collect()
}

fn all_on(ctx: &Ctx<'_>) -> Vec<Vec<bool>> {
    ctx.geometry
        .iter()
        .map(|dev| vec![true; dev.num_transducers()])
        .collect()
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);
    let mut sine150 = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut sine150)?;
    send_pattern_mod(ctx, &focus, &sine150, SamplingConfig::FREQ_4K).await?;
    wait_enter("A focus is formed 150 mm above the centre of each device").await;

    ctx.send(SetOutputMask {
        masks: &half_mask(ctx, true),
    })
    .await?;
    wait_enter("Only the left half of device 0 and the right half of the others output").await;

    ctx.send(SetOutputMask {
        masks: &all_on(ctx),
    })
    .await?;
    send_pattern_mod(ctx, &focus, &sine150, SamplingConfig::FREQ_4K).await?;
    wait_enter("A focus is formed 150 mm above the centre of each device again").await;

    ctx.send(SetOutputMask {
        masks: &half_mask(ctx, false),
    })
    .await?;
    wait_enter("Only the right half of device 0 and the left half of the others output").await;
    Ok(())
}
