use anyhow::Result;

use crate::Ctx;
use crate::io::wait_enter;

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    let errors = ctx.client.read_error_detail().await?;
    for (i, code) in errors.iter().enumerate() {
        println!("  device[{i}] error detail: {code:#04x}");
    }

    let faulted: Vec<usize> = errors
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| (c != 0).then_some(i))
        .collect();
    if faulted.is_empty() {
        println!("All devices report no error (code=0x00)");
    } else {
        eprintln!("Devices reporting an error: {faulted:?}");
    }

    wait_enter("You have checked the error-detail register values").await;
    Ok(())
}
