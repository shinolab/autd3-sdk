use std::sync::Arc;
use std::time::{Duration, Instant};

use autd3_rs_core::{CycleOutcome, Link, RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::diagnostics::{CycleDiagnostics, store_cycle_diagnostics};
use crate::error::EtherCrabLinkError;
use crate::state_check::StateChecker;

use super::EtherCrabLink;

impl Link for EtherCrabLink {
    type Error = EtherCrabLinkError;
    type Checker = StateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn stats(&self) -> autd3_rs_core::LinkStats {
        self.stats.clone()
    }

    fn state_checker(&self) -> StateChecker {
        StateChecker::new(
            &self.transport.maindevice_arc(),
            self.addresses.clone(),
            Arc::clone(&self.diagnostics),
        )
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        let Some(group) = self.group.as_ref() else {
            return Err(EtherCrabLinkError::Closed);
        };
        let maindevice = self.transport.maindevice();

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

        for (subdevice, frame) in group.groups.iter().flat_map(|g| g.iter(maindevice)).zip(tx) {
            let mut outputs = subdevice.outputs_raw_mut();
            let len = outputs.len().min(frame.len());
            outputs[..len].copy_from_slice(&frame[..len]);
        }

        let cycle_start = Instant::now();
        let resp = match self.handle.block_on(group.tx_rx_dc(maindevice)) {
            Ok(resp) => resp,
            Err(
                e @ (ethercrab::error::Error::Timeout(_)
                | ethercrab::error::Error::WorkingCounter { .. }),
            ) => {
                self.next_at = None;
                self.stats.record_lost_cycle();
                let tx_rx_duration = cycle_start.elapsed();
                self.store_failed_cycle_diagnostics(deadline_overrun, tx_rx_duration);
                if self.rx_was_valid {
                    tracing::warn!(
                        ?deadline_overrun,
                        ?tx_rx_duration,
                        expected_wkc = self.expected_wkc,
                        "bus cycle failed ({e}); link may be lost",
                    );
                    self.rx_was_valid = false;
                }
                return Ok(CycleOutcome { rx_valid: false });
            }
            Err(e) => return Err(e.into()),
        };
        let tx_rx_duration = cycle_start.elapsed();
        self.next_at = Some(cycle_start + resp.next_cycle_wait);

        let all_op = resp.all_op;
        let rx_valid = resp.working_counter == self.expected_wkc && all_op;
        self.store_cycle_diagnostics(
            deadline_overrun,
            tx_rx_duration,
            resp.working_counter,
            all_op,
            rx_valid,
            resp.dc_system_time,
            resp.next_cycle_wait,
            resp.cycle_start_offset,
        );
        if !rx_valid {
            self.stats.record_stale_cycle();
        }
        if rx_valid != self.rx_was_valid {
            if rx_valid {
                tracing::info!("bus recovered, rx valid again");
            } else {
                tracing::warn!(
                    working_counter = resp.working_counter,
                    expected_wkc = self.expected_wkc,
                    all_op,
                    ?deadline_overrun,
                    ?tx_rx_duration,
                    dc_system_time_ns = resp.dc_system_time,
                    next_cycle_wait = ?resp.next_cycle_wait,
                    cycle_start_offset = ?resp.cycle_start_offset,
                    "stale cycle: slaves did not process this cycle",
                );
            }
            self.rx_was_valid = rx_valid;
        }

        for (subdevice, frame) in group
            .groups
            .iter()
            .flat_map(|g| g.iter(maindevice))
            .zip(rx.iter_mut())
        {
            let inputs = subdevice.inputs_raw();
            let len = frame.len().min(inputs.len());
            frame[..len].copy_from_slice(&inputs[..len]);
        }

        Ok(CycleOutcome { rx_valid })
    }
}

impl EtherCrabLink {
    fn store_failed_cycle_diagnostics(&self, deadline_overrun: Duration, tx_rx_duration: Duration) {
        let previous_samples =
            crate::diagnostics::load_cycle_diagnostics(&self.diagnostics).samples;
        store_cycle_diagnostics(
            &self.diagnostics,
            CycleDiagnostics {
                samples: previous_samples.saturating_add(1),
                deadline_overrun,
                tx_rx_duration,
                expected_wkc: self.expected_wkc,
                working_counter: None,
                all_op: None,
                rx_valid: false,
                tx_rx_succeeded: false,
                dc_system_time_ns: None,
                next_cycle_wait: None,
                cycle_start_offset: None,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn store_cycle_diagnostics(
        &self,
        deadline_overrun: Duration,
        tx_rx_duration: Duration,
        working_counter: u16,
        all_op: bool,
        rx_valid: bool,
        dc_system_time_ns: u64,
        next_cycle_wait: Duration,
        cycle_start_offset: Duration,
    ) {
        let previous_samples =
            crate::diagnostics::load_cycle_diagnostics(&self.diagnostics).samples;
        store_cycle_diagnostics(
            &self.diagnostics,
            CycleDiagnostics {
                samples: previous_samples.saturating_add(1),
                deadline_overrun,
                tx_rx_duration,
                expected_wkc: self.expected_wkc,
                working_counter: Some(working_counter),
                all_op: Some(all_op),
                rx_valid,
                tx_rx_succeeded: true,
                dc_system_time_ns: Some(dc_system_time_ns),
                next_cycle_wait: Some(next_cycle_wait),
                cycle_start_offset: Some(cycle_start_offset),
            },
        );
    }
}
