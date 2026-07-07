use anyhow::Result;

use autd3_rs::commands::Pattern;
use autd3_rs::units::Hz;
use autd3_rs::value::{PatternBank, SamplingConfig};
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};
use autd3_rs_pattern::null;

use crate::Ctx;
use crate::cases::pattern_util::{
    change_pattern_bank, focus_at, report_fpga_state, send_pattern_mod, write_pattern_bank,
};
use crate::io::wait_enter;

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);
    let mut nullbuf = ctx.geometry.pattern_buffer();
    null(&mut nullbuf);

    let mut modbuf = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut modbuf)?;
    send_pattern_mod(ctx, &focus, &modbuf, SamplingConfig::FREQ_4K).await?;
    wait_enter("A focus is formed 150 mm above the centre of each device").await;
    report_fpga_state(ctx, "B0 focus", None, Some(PatternBank::B0), Some(true)).await?;

    ctx.send(Pattern::with_bank(PatternBank::B1, &nullbuf))
        .await?;
    wait_enter("The focus disappeared").await;
    report_fpga_state(ctx, "B1 null", None, Some(PatternBank::B1), Some(true)).await?;

    change_pattern_bank(ctx, PatternBank::B0).await?;
    wait_enter("The focus is shown again").await;
    report_fpga_state(ctx, "back to B0", None, Some(PatternBank::B0), Some(true)).await?;

    write_pattern_bank(ctx, PatternBank::B1, &nullbuf).await?;
    wait_enter("The focus is still shown").await;
    report_fpga_state(
        ctx,
        "B0 stays active",
        None,
        Some(PatternBank::B0),
        Some(true),
    )
    .await?;

    change_pattern_bank(ctx, PatternBank::B1).await?;
    wait_enter("The focus disappeared").await;
    report_fpga_state(ctx, "switch to B1", None, Some(PatternBank::B1), Some(true)).await?;
    Ok(())
}
