use anyhow::Result;

use autd3_rs::commands::ForceFan;

use crate::Ctx;
use crate::io::wait_enter;

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    ctx.send(ForceFan { value: true }).await?;
    wait_enter("The fan is spinning").await;

    ctx.send(ForceFan { value: false }).await?;
    wait_enter("The fan has stopped").await;
    Ok(())
}
