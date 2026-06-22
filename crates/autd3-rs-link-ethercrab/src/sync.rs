use std::time::Duration;

use ethercrab::subdevice_group::{NoDc, PreOpPdi};
use ethercrab::{MainDevice, RegisterAddress};
use futures_util::future::join_all;

use crate::error::EtherCrabLinkError;
use crate::link::Groups;
use crate::timer;

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

pub(crate) async fn wait_for_align(
    group: &Groups<PreOpPdi, NoDc>,
    maindevice: &MainDevice<'_>,
    sync_tolerance: Duration,
    sync_timeout: Duration,
) -> Result<(), EtherCrabLinkError> {
    tracing::info!(
        ?sync_tolerance,
        ?sync_timeout,
        "waiting for DC clocks to align"
    );

    let mut averages = vec![Ema::new(); group.num_devices()];
    let start = std::time::Instant::now();
    let mut last_check = start;
    loop {
        for result in join_all(
            group
                .groups
                .iter()
                .map(|g| g.tx_rx_sync_system_time(maindevice)),
        )
        .await
        {
            result?;
        }

        if last_check.elapsed() >= CHECK_INTERVAL {
            last_check = std::time::Instant::now();

            let mut max_deviation = Duration::ZERO;
            for (subdevice, ema) in group
                .groups
                .iter()
                .flat_map(|g| g.iter(maindevice))
                .zip(averages.iter_mut())
            {
                let diff_ns = match subdevice
                    .register_read::<u32>(RegisterAddress::DcSystemTimeDifference)
                    .await
                {
                    Ok(raw) => system_time_difference_ns(raw),
                    Err(ethercrab::error::Error::WorkingCounter { .. }) => 0.0,
                    Err(e) => return Err(e.into()),
                };
                let smoothed_ns = ema.push(diff_ns).abs();
                max_deviation = max_deviation.max(Duration::from_secs_f64(smoothed_ns * 1e-9));
            }

            tracing::debug!(?max_deviation, "DC system time deviation");
            if max_deviation < sync_tolerance {
                tracing::info!(elapsed = ?start.elapsed(), "DC clocks aligned");
                return Ok(());
            }
            if start.elapsed() > sync_timeout {
                return Err(EtherCrabLinkError::SyncTimeout(max_deviation));
            }
        }

        timer::async_sleep(LOOP_SLEEP).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ema_smooths_with_alpha_0_2() {
        let mut ema = Ema::new();
        assert!((ema.push(10.0) - 10.0).abs() < 1e-9);
        assert!((ema.push(20.0) - 12.0).abs() < 1e-9);
        assert!((ema.push(30.0) - 15.6).abs() < 1e-9);
    }

    #[test]
    fn dcsysdiff_decodes_sign_magnitude() {
        assert!((system_time_difference_ns(100) - 100.0).abs() < f64::EPSILON);
        assert!((system_time_difference_ns(0x8000_0064) - -100.0).abs() < f64::EPSILON);
        assert!((system_time_difference_ns(0) - 0.0).abs() < f64::EPSILON);
    }
}
