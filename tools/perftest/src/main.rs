mod cli;
mod mem;
mod report;
mod run;
mod stats;

use anyhow::Result;
use autd3_rs::PerfTuning;
use clap::Parser;

use crate::cli::Cli;
use crate::report::{print_mem, print_summary, write_csv};
use crate::run::{RunOutput, run};
use crate::stats::Summary;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(msg) = cli.validate() {
        anyhow::bail!(msg);
    }

    let _tuning = (!cli.no_win_perf_tune).then(|| {
        let tuning = PerfTuning::apply();
        eprintln!(
            "perf-tune: timer={}, priority={}",
            if tuning.timer_boosted() {
                "1ms"
            } else {
                "default"
            },
            if tuning.high_priority() {
                "HIGH"
            } else {
                "default"
            },
        );
        tuning
    });

    let output = Box::pin(run(&cli)).await?;
    let RunOutput {
        samples,
        warmup,
        elapsed,
        frame_bytes,
        stale_cycles,
        lost_cycles,
        mem,
    } = output;

    let drop = usize::try_from(warmup).unwrap_or(samples.len());
    let measured = &samples[drop..];

    let summary = Summary::from_samples(measured, frame_bytes, elapsed, stale_cycles, lost_cycles);
    print_summary(&summary);
    if let Some(mem) = &mem {
        print_mem(mem);
    }

    if let Some(path) = &cli.csv {
        if let Err(e) = write_csv(path, &samples) {
            eprintln!("warning: failed to write CSV to {}: {e}", path.display());
        } else {
            println!("\nCSV written: {}", path.display());
        }
    }

    Ok(())
}
