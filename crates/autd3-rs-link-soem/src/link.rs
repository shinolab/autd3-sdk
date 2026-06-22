use std::sync::Arc;
use std::time::{Duration, Instant};

use autd3_rs_core::{CycleOutcome, Link, LinkStats, RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::adapters;
use crate::context::{Context, EC_TIMEOUTRET_US};
use crate::diagnostics::{
    CycleDiagnostics, SharedCycleDiagnostics, new_shared_cycle_diagnostics, store_cycle_diagnostics,
};
use crate::error::SoemLinkError;
use crate::option::SoemLinkOptionFull;
use crate::state::AlState;
use crate::state_check::StateChecker;
use crate::sync;
use crate::timer::TimerResolutionGuard;

pub(crate) const SUBDEVICE_NAME: &str = "AUTD";
const WKC_STABLE_CYCLES: u32 = 5;
const TIMER_RESOLUTION_MS: u32 = 1;

impl autd3_rs_core::IntoLink for crate::option::SoemLinkOption {
    type Link = SoemLink;

    async fn into_link(self) -> Result<SoemLink, autd3_rs_core::Error> {
        SoemLinkOptionFull::from(self).into_link().await
    }
}

impl autd3_rs_core::IntoLink for SoemLinkOptionFull {
    type Link = SoemLink;

    async fn into_link(self) -> Result<SoemLink, autd3_rs_core::Error> {
        tokio::task::spawn_blocking(move || SoemLink::open(self))
            .await
            .map_err(|e| autd3_rs_core::Error::Link(format!("link open task panicked: {e}")))?
            .map_err(|e| autd3_rs_core::Error::Link(e.to_string()))
    }
}

fn next_cycle_wait(dc_time_ns: i64, cycle_ns: i64, shift_ns: i64) -> Duration {
    let phase = dc_time_ns.rem_euclid(cycle_ns);
    #[allow(clippy::cast_sign_loss)]
    Duration::from_nanos(((cycle_ns - phase) + shift_ns) as u64)
}

#[allow(clippy::cast_possible_truncation)]
fn as_us_i32(d: Duration) -> i32 {
    i32::try_from(d.as_micros()).unwrap_or(i32::MAX)
}

pub struct SoemLink {
    ctx: Arc<Context>,
    _iomap: Box<[u8]>,
    num_devices: usize,
    expected_wkc: i32,
    cycle_ns: i64,
    shift_ns: i64,
    next_at: Option<Instant>,
    rx_was_valid: bool,
    stats: LinkStats,
    shutdown_done: bool,
    diagnostics: SharedCycleDiagnostics,
    _timer_resolution: TimerResolutionGuard,
}

impl SoemLink {
    pub fn open(option: impl Into<SoemLinkOptionFull>) -> Result<Self, SoemLinkError> {
        let option: SoemLinkOptionFull = option.into();
        tracing::debug!(?option, "opening SoemLink");

        let timer_resolution = TimerResolutionGuard::new(TIMER_RESOLUTION_MS);

        let interface = if let Some(interface) = option.interface.name() {
            interface.to_owned()
        } else {
            tracing::info!("no interface specified, looking for AUTD devices");
            let interface = adapters::lookup_autd(&option)?;
            tracing::info!("found AUTD devices on {interface}");
            interface
        };

        let ctx = Arc::new(Context::new(option.sync0_period, option.sync0_shift));
        let diagnostics = new_shared_cycle_diagnostics();
        ctx.init(&interface)?;
        tracing::info!("initialized SOEM on {interface}");

        let num_devices = ctx.config_init();
        if num_devices == 0 {
            return Err(SoemLinkError::DeviceNotFound);
        }
        for index in 0..num_devices {
            let name = ctx.slave_name(index);
            if name != SUBDEVICE_NAME {
                return Err(SoemLinkError::NotAutdDevice { index, name });
            }
        }
        tracing::info!("found {num_devices} AUTD device(s) on {interface}");

        tracing::info!(sync0_period = ?option.sync0_period, "configuring DC");
        ctx.set_po2so_hooks();
        if !ctx.configdc() {
            return Err(SoemLinkError::DcConfigFailed);
        }

        sync::wait_for_align(&ctx, option.sync_tolerance, option.sync_timeout)?;

        let mut iomap =
            vec![0u8; (1 + TX_FRAME_BYTES + RX_FRAME_BYTES) * num_devices].into_boxed_slice();
        // SAFETY: `iomap` is heap storage owned by the returned link, which
        // also owns the last `Arc<Context>`; it stays valid and in place for
        // every process-data exchange.
        let mapped = unsafe { ctx.config_map_group(iomap.as_mut_ptr().cast()) };
        tracing::debug!(mapped, "mapped process data image");

        let state = ctx.statecheck(0, AlState::SAFE_OP, as_us_i32(option.state_timeout));
        if state != AlState::SAFE_OP {
            return Err(SoemLinkError::StateTransitionFailed {
                expected: AlState::SAFE_OP,
                actual: state,
            });
        }
        tracing::info!("all devices are in SAFE-OP");

        let expected_wkc = ctx.expected_wkc();

        wait_for_op(
            &ctx,
            expected_wkc,
            option.send_cycle,
            option.sync0_shift,
            option.op_wait_timeout,
        )?;

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let cycle_ns = option.send_cycle.as_nanos() as i64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let shift_ns = option.sync0_shift.as_nanos() as i64;
        Ok(Self {
            ctx,
            _iomap: iomap,
            num_devices,
            expected_wkc,
            cycle_ns,
            shift_ns,
            next_at: None,
            rx_was_valid: true,
            stats: LinkStats::default(),
            shutdown_done: false,
            diagnostics,
            _timer_resolution: timer_resolution,
        })
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn shutdown(&mut self) {
        if self.shutdown_done {
            return;
        }
        self.shutdown_done = true;
        tracing::info!("stopping Sync0 and transitioning devices to INIT");
        for index in 0..self.num_devices {
            self.ctx.dcsync0_off(index);
        }
        self.ctx.request_state(None, AlState::INIT);
    }
}

impl Drop for SoemLink {
    fn drop(&mut self) {
        self.shutdown();
    }
}

const OP_RECOVERY_INTERVAL: Duration = Duration::from_millis(100);
const OP_WARMUP_CYCLES: u32 = 200;

fn wait_for_op(
    ctx: &Context,
    expected_wkc: i32,
    cycle: Duration,
    shift: Duration,
    timeout: Duration,
) -> Result<(), SoemLinkError> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let cycle_ns = cycle.as_nanos() as i64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let shift_ns = shift.as_nanos() as i64;
    let start = Instant::now();
    let mut last_recovery = start;
    let mut stable_cycles: u32 = 0;
    let mut cycles: u32 = 0;
    let mut op_requested = false;
    loop {
        let cycle_start = Instant::now();
        ctx.send_processdata();
        let wkc = ctx.receive_processdata(EC_TIMEOUTRET_US);
        cycles = cycles.saturating_add(1);

        if !op_requested && cycles >= OP_WARMUP_CYCLES {
            ctx.request_state(None, AlState::OP);
            op_requested = true;
            tracing::info!("requested OP, waiting for all devices");
        }

        if op_requested && wkc == expected_wkc {
            stable_cycles += 1;
            if stable_cycles >= WKC_STABLE_CYCLES {
                if all_op(ctx) {
                    tracing::info!(
                        expected_wkc,
                        elapsed = ?start.elapsed(),
                        "all devices entered OP",
                    );
                    return Ok(());
                }
                stable_cycles = 0;
            }
        } else {
            tracing::trace!(wkc, "devices are not in OP with a stable wkc yet");
            stable_cycles = 0;
        }

        if op_requested && stable_cycles == 0 && last_recovery.elapsed() >= OP_RECOVERY_INTERVAL {
            last_recovery = Instant::now();
            recover_op_transition(ctx);
        }

        if start.elapsed() >= timeout {
            ctx.read_state();
            for index in 0..ctx.num_slaves() {
                tracing::error!(
                    device = index,
                    state = %ctx.slave_state(index),
                    al_status = %ctx.al_status_string(index),
                    "device state at OP timeout",
                );
            }
            tracing::error!(
                elapsed = ?start.elapsed(),
                "timeout waiting for OP: devices did not reach OP within {timeout:?}",
            );
            return Err(SoemLinkError::OpTimeout);
        }
        let deadline = cycle_start + next_cycle_wait(ctx.dc_time(), cycle_ns, shift_ns);
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
    }
}

fn all_op(ctx: &Context) -> bool {
    ctx.read_state();
    (0..ctx.num_slaves()).all(|index| ctx.slave_state(index).is_op())
}

fn recover_op_transition(ctx: &Context) {
    ctx.read_state();
    for index in 0..ctx.num_slaves() {
        let al = ctx.slave_state(index);
        if al.is_safe_op() && al.is_error() {
            tracing::debug!(
                device = index,
                al_status = %ctx.al_status_string(index),
                "device refused OP; acknowledging the error",
            );
            ctx.request_state(Some(index), AlState::SAFE_OP_ACK);
        } else if al.is_safe_op() {
            tracing::debug!(device = index, "re-requesting OP");
            ctx.request_state(Some(index), AlState::OP);
        }
    }
}

impl Link for SoemLink {
    type Error = SoemLinkError;
    type Checker = StateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn stats(&self) -> LinkStats {
        self.stats.clone()
    }

    fn state_checker(&self) -> StateChecker {
        StateChecker::new(&self.ctx, self.num_devices, Arc::clone(&self.diagnostics))
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        let deadline_overrun = if let Some(deadline) = self.next_at {
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
                Duration::ZERO
            } else {
                now.duration_since(deadline)
            }
        } else {
            Duration::ZERO
        };

        for (index, frame) in tx.iter().enumerate() {
            self.ctx.copy_outputs(index, frame);
        }

        let cycle_start = Instant::now();
        self.ctx.send_processdata();
        let wkc = self.ctx.receive_processdata(EC_TIMEOUTRET_US);
        let tx_rx_duration = cycle_start.elapsed();
        if wkc <= 0 {
            self.next_at = None;
            self.stats.record_lost_cycle();
            let previous_samples =
                crate::diagnostics::load_cycle_diagnostics(&self.diagnostics).samples;
            store_cycle_diagnostics(
                &self.diagnostics,
                CycleDiagnostics {
                    samples: previous_samples.saturating_add(1),
                    deadline_overrun,
                    tx_rx_duration,
                    expected_wkc: self.expected_wkc,
                    working_counter: Some(wkc),
                    rx_valid: false,
                    tx_rx_succeeded: false,
                    dc_time_ns: None,
                    next_cycle_wait: None,
                    dc_phase: None,
                },
            );
            if self.rx_was_valid {
                tracing::warn!(
                    wkc,
                    ?deadline_overrun,
                    ?tx_rx_duration,
                    expected_wkc = self.expected_wkc,
                    "bus cycle failed; link may be lost",
                );
                self.rx_was_valid = false;
            }
            return Ok(CycleOutcome { rx_valid: false });
        }

        let dc_time_ns = self.ctx.dc_time();
        let wait = next_cycle_wait(dc_time_ns, self.cycle_ns, self.shift_ns);
        self.next_at = Some(cycle_start + wait);

        let rx_valid = wkc == self.expected_wkc;
        let previous_samples =
            crate::diagnostics::load_cycle_diagnostics(&self.diagnostics).samples;
        #[allow(clippy::cast_sign_loss)]
        let dc_phase = Duration::from_nanos(dc_time_ns.rem_euclid(self.cycle_ns) as u64);
        store_cycle_diagnostics(
            &self.diagnostics,
            CycleDiagnostics {
                samples: previous_samples.saturating_add(1),
                deadline_overrun,
                tx_rx_duration,
                expected_wkc: self.expected_wkc,
                working_counter: Some(wkc),
                rx_valid,
                tx_rx_succeeded: true,
                dc_time_ns: Some(dc_time_ns),
                next_cycle_wait: Some(wait),
                dc_phase: Some(dc_phase),
            },
        );
        if !rx_valid {
            self.stats.record_stale_cycle();
        }
        if rx_valid != self.rx_was_valid {
            if rx_valid {
                tracing::info!("bus recovered, rx valid again");
            } else {
                tracing::warn!(
                    wkc,
                    expected_wkc = self.expected_wkc,
                    ?deadline_overrun,
                    ?tx_rx_duration,
                    dc_time_ns,
                    next_cycle_wait = ?wait,
                    dc_phase = ?dc_phase,
                    "stale cycle: slaves did not process this cycle",
                );
            }
            self.rx_was_valid = rx_valid;
        }

        for (index, frame) in rx.iter_mut().enumerate() {
            self.ctx.copy_inputs(index, frame);
        }

        Ok(CycleOutcome { rx_valid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_cycle_wait_targets_sync0_edge() {
        let cycle = 1_000_000i64;
        assert_eq!(next_cycle_wait(0, cycle, 0), Duration::from_millis(1));
        assert_eq!(
            next_cycle_wait(cycle / 2, cycle, 0),
            Duration::from_micros(500)
        );
        assert_eq!(
            next_cycle_wait(42 * cycle + cycle / 2, cycle, 0),
            Duration::from_micros(500)
        );
    }

    #[test]
    fn next_cycle_wait_honors_sync0_shift() {
        let cycle = 1_000_000i64;
        let shift = 250_000i64;
        assert_eq!(
            next_cycle_wait(0, cycle, shift),
            Duration::from_micros(1_250)
        );
        assert_eq!(
            next_cycle_wait(cycle - 1, cycle, shift),
            Duration::from_nanos(250_001)
        );
    }
}
