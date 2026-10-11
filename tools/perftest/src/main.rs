mod cli;
mod mem;
mod report;
mod run;
mod stats;

use anyhow::Result;
use autd3_rs::rt::{LogWriter, TracingOption, init_tracing};
use clap::Parser;

use crate::cli::Cli;
use crate::report::{print_mem, print_summary, print_telemetry};
use crate::run::{RunOutput, run};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption {
        default_filter: "info",
        writer: LogWriter::Stderr,
    });

    let cli = Cli::parse();
    if let Err(msg) = cli.validate() {
        anyhow::bail!(msg);
    }

    let output = Box::pin(run(&cli)).await?;
    let RunOutput {
        summary,
        stopped_on_error,
        driver_closed,
        mem,
        telemetry_start,
        telemetry_end,
    } = output;

    print_summary(&summary);
    print_telemetry(&telemetry_start, &telemetry_end);
    if let Some(mem) = &mem {
        print_mem(mem);
    }

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
