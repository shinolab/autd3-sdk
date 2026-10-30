mod cli;
mod report;
mod run;
mod stats;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;
use crate::report::{print_summary, print_telemetry};
use crate::run::{RunOutput, run};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(msg) = cli.validate() {
        anyhow::bail!(msg);
    }

    let output = Box::pin(run(&cli)).await?;
    let RunOutput {
        summary,
        stopped_on_error,
        driver_closed,
        telemetry_start,
        telemetry_end,
    } = output;

    print_summary(&summary);
    print_telemetry(&telemetry_start, &telemetry_end);

    if let Some((index, status)) = stopped_on_error {
        anyhow::bail!("stopped at send #{index}: {status:?} (--stop-on-error)");
    }
    if driver_closed {
        anyhow::bail!(
            "the client closed before the run finished; \
             the summary covers only what was sent until then"
        );
    }

    Ok(())
}
