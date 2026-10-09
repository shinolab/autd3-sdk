mod cli;
mod mem;
mod report;
mod run;
mod stats;

use anyhow::Result;
use autd3_rs::rt::{LogWriter, TracingOption, init_tracing};
use clap::Parser;

use crate::cli::Cli;
use crate::report::{print_mem, print_summary, write_csv};
use crate::run::{RunOutput, run};
use crate::stats::Summary;

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
        samples,
        sends,
        stopped_on_error,
        driver_closed,
        elapsed,
        frame_bytes,
        missed_replies,
        driver_ack,
        mem,
    } = output;

    if sends > samples.len() as u64 {
        eprintln!(
            "warning: recorded only the first {} of {sends} sends (--max-samples); \
             the summary and CSV cover that prefix only \
             (except the driver ack latency, which covers the whole run)",
            samples.len(),
        );
    }

    let drop = usize::try_from(cli.warmup)
        .unwrap_or(samples.len())
        .min(samples.len());
    let measured = &samples[drop..];

    if let Some(path) = &cli.csv {
        if let Err(e) = write_csv(path, &samples) {
            eprintln!("warning: failed to write CSV to {}: {e}", path.display());
        } else {
            println!("\nCSV written: {}", path.display());
        }
    }

    let summary = Summary::from_samples(measured, frame_bytes, elapsed, missed_replies, driver_ack);
    print_summary(&summary);
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
