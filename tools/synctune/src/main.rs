mod cli;
mod grid;
mod monitor;
mod report;
mod run;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use autd3_rs::PerfTuning;
use clap::Parser;

use crate::cli::{Cli, Command, Common, MeasureArgs, TuneArgs};
use crate::grid::{Candidate, candidates, select_best};
use crate::monitor::CandidateResult;
use crate::run::measure_candidate;

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
    let common = match &cli.cmd {
        Command::Measure(a) => {
            if let Err(msg) = a.validate() {
                anyhow::bail!(msg);
            }
            a.common.clone()
        }
        Command::Tune(a) => {
            if let Err(msg) = a.validate() {
                anyhow::bail!(msg);
            }
            a.common.clone()
        }
    };

    let _tuning = (!common.no_win_perf_tune).then(|| {
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

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    match cli.cmd {
        Command::Measure(a) => run_measure(&a, &shutdown).await,
        Command::Tune(a) => run_tune(&a, &shutdown).await,
    }
}

async fn run_measure(args: &MeasureArgs, shutdown: &Arc<AtomicBool>) -> Result<()> {
    let cand = Candidate {
        period: Duration::from_micros(args.cycle_us),
        shift_percent: args.shift_percent,
    };
    eprintln!(
        "measuring period={}us shift={}% (warmup={:?}, dwell={:?})...",
        args.cycle_us, args.shift_percent, args.common.warmup, args.common.dwell,
    );
    let result = Box::pin(measure_candidate(&args.common, cand, shutdown)).await?;
    report::print_measure(&result, &args.common);
    write_csv_if_requested(&args.common, std::slice::from_ref(&result));
    Ok(())
}

async fn run_tune(args: &TuneArgs, shutdown: &Arc<AtomicBool>) -> Result<()> {
    let cands = candidates(args);
    let total = cands.len();
    eprintln!(
        "tuning {} candidate(s): period {}..={}us step {}, shift {}..={}% step {} \
(warmup={:?}, dwell={:?} each)",
        total,
        args.period_min,
        args.period_max,
        args.period_step,
        args.shift_min,
        args.shift_max,
        args.shift_step,
        args.common.warmup,
        args.common.dwell,
    );

    let mut results: Vec<CandidateResult> = Vec::with_capacity(total);
    for (i, cand) in cands.into_iter().enumerate() {
        if shutdown.load(Ordering::Relaxed) {
            eprintln!("interrupted — stopping sweep ({i}/{total} done)");
            break;
        }
        eprintln!(
            "[{}/{}] period={}us shift={}% ...",
            i + 1,
            total,
            cand.period.as_micros(),
            cand.shift_percent,
        );
        let result = Box::pin(measure_candidate(&args.common, cand, shutdown)).await?;
        eprintln!(
            "    -> {} op_ratio={:.2}% drops={}",
            result.status.label(),
            result.op_ratio() * 100.0,
            result.drop_events,
        );
        results.push(result);
    }

    let best = select_best(&results);
    report::print_table(&results, best);
    report::print_best(&results, best, &args.common);
    write_csv_if_requested(&args.common, &results);
    Ok(())
}

fn write_csv_if_requested(common: &Common, results: &[CandidateResult]) {
    if let Some(path) = &common.csv {
        match report::write_csv(path, results) {
            Ok(()) => println!("\nCSV written: {}", path.display()),
            Err(e) => eprintln!("warning: failed to write CSV to {}: {e}", path.display()),
        }
    }
}

fn spawn_signal_listener(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            flag.store(true, Ordering::Relaxed);
            eprintln!("\nCtrl+C received — stopping after the current candidate...");
        }
    });
}
