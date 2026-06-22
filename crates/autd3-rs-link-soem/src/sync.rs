use std::time::{Duration, Instant};

use crate::context::Context;
use crate::error::SoemLinkError;

const CHECK_INTERVAL: Duration = Duration::from_millis(10);
const LOOP_SLEEP: Duration = Duration::from_millis(1);
const EMA_ALPHA: f64 = 0.2;

#[derive(Clone, Copy)]
struct Ema {
    current: Option<f64>,
}

impl Ema {
    const fn new() -> Self {
        Self { current: None }
    }

    fn push(&mut self, value: f64) -> f64 {
        let current = self.current.get_or_insert(value);
        *current = EMA_ALPHA * value + (1.0 - EMA_ALPHA) * *current;
        *current
    }
}

// DCSYSDIFF is sign+magnitude, not two's complement.
// See RZ/T1 Group User's Manual: Hardware, 30.17.2.5.
fn system_time_difference_ns(raw: u32) -> f64 {
    const MASK: u32 = 0x7FFF_FFFF;
    let magnitude = f64::from(raw & MASK);
    if raw & !MASK == 0 {
        magnitude
    } else {
        -magnitude
    }
}

pub(crate) fn wait_for_align(
    ctx: &Context,
    sync_tolerance: Duration,
    sync_timeout: Duration,
) -> Result<(), SoemLinkError> {
    tracing::info!(
        ?sync_tolerance,
        ?sync_timeout,
        "waiting for DC clocks to align"
    );

    let mut averages = vec![Ema::new(); ctx.num_slaves()];
    let start = Instant::now();
    let mut last_check = start;
    loop {
        ctx.distribute_dc_time();

        if last_check.elapsed() >= CHECK_INTERVAL {
            last_check = Instant::now();

            let mut max_deviation = Duration::ZERO;
            for (index, ema) in averages.iter_mut().enumerate().skip(1) {
                let diff_ns = ctx
                    .dc_system_time_difference(index)
                    .map_or(0.0, system_time_difference_ns);
                let smoothed_ns = ema.push(diff_ns).abs();
                max_deviation = max_deviation.max(Duration::from_secs_f64(smoothed_ns * 1e-9));
            }

            tracing::debug!(?max_deviation, "DC system time deviation");
            if max_deviation < sync_tolerance {
                tracing::info!(elapsed = ?start.elapsed(), "DC clocks aligned");
                return Ok(());
            }
            if start.elapsed() > sync_timeout {
                return Err(SoemLinkError::SyncTimeout(max_deviation));
            }
        }

        std::thread::sleep(LOOP_SLEEP);
    }
}
