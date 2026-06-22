use std::sync::{Arc, Weak};

use autd3_rs_core::{DeviceState, LinkStatus};

use crate::context::Context;
use crate::diagnostics::{SharedCycleDiagnostics, load_cycle_diagnostics};
use crate::error::SoemLinkError;
use crate::state::AlState;

pub struct StateChecker {
    ctx: Weak<Context>,
    states: Vec<DeviceState>,
    recoveries: u64,
    diagnostics: SharedCycleDiagnostics,
}

impl StateChecker {
    pub(crate) fn new(
        ctx: &Arc<Context>,
        num_devices: usize,
        diagnostics: SharedCycleDiagnostics,
    ) -> Self {
        Self {
            ctx: Arc::downgrade(ctx),
            states: vec![DeviceState::Op; num_devices],
            recoveries: 0,
            diagnostics,
        }
    }

    pub fn check(&mut self) -> Result<LinkStatus, SoemLinkError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(SoemLinkError::Closed);
        };
        let was_all_op = self.states.iter().all(|s| *s == DeviceState::Op);
        ctx.read_state();
        for (device, state) in self.states.iter_mut().enumerate() {
            let al = ctx.slave_state(device);
            let new_state = if al.is_op() {
                DeviceState::Op
            } else if al.is_none() {
                DeviceState::Lost
            } else if al.is_safe_op() && al.is_error() {
                ctx.request_state(Some(device), AlState::SAFE_OP_ACK);
                DeviceState::SafeOpError
            } else if al.is_safe_op() {
                ctx.request_state(Some(device), AlState::OP);
                DeviceState::SafeOp
            } else {
                DeviceState::Other(al.state_bits())
            };
            if new_state != *state {
                let diagnostics = load_cycle_diagnostics(&self.diagnostics);
                match new_state {
                    DeviceState::Op => tracing::info!(device, "device is back in OP"),
                    DeviceState::SafeOpError => {
                        tracing::warn!(
                            device,
                            al_status = %ctx.al_status_string(device),
                            diagnostic_samples = diagnostics.samples,
                            deadline_overrun = ?diagnostics.deadline_overrun,
                            tx_rx_duration = ?diagnostics.tx_rx_duration,
                            expected_wkc = diagnostics.expected_wkc,
                            working_counter = ?diagnostics.working_counter,
                            rx_valid = diagnostics.rx_valid,
                            tx_rx_succeeded = diagnostics.tx_rx_succeeded,
                            dc_time_ns = ?diagnostics.dc_time_ns,
                            next_cycle_wait = ?diagnostics.next_cycle_wait,
                            dc_phase = ?diagnostics.dc_phase,
                            "device is in SAFE-OP + ERROR, acknowledged",
                        );
                    }
                    DeviceState::SafeOp => {
                        tracing::warn!(
                            device,
                            al_status = %ctx.al_status_string(device),
                            diagnostic_samples = diagnostics.samples,
                            deadline_overrun = ?diagnostics.deadline_overrun,
                            tx_rx_duration = ?diagnostics.tx_rx_duration,
                            expected_wkc = diagnostics.expected_wkc,
                            working_counter = ?diagnostics.working_counter,
                            rx_valid = diagnostics.rx_valid,
                            tx_rx_succeeded = diagnostics.tx_rx_succeeded,
                            dc_time_ns = ?diagnostics.dc_time_ns,
                            next_cycle_wait = ?diagnostics.next_cycle_wait,
                            dc_phase = ?diagnostics.dc_phase,
                            "device is in SAFE-OP, requested OP",
                        );
                    }
                    DeviceState::Lost => tracing::warn!(device, "device is lost"),
                    DeviceState::Other(_) => {
                        tracing::warn!(device, state = %new_state, "device is in an unrecoverable state");
                    }
                }
                *state = new_state;
            }
        }
        if !was_all_op && self.states.iter().all(|s| *s == DeviceState::Op) {
            self.recoveries += 1;
            tracing::info!("all devices resumed OP");
        }
        Ok(LinkStatus {
            devices: self.states.clone(),
            recoveries: self.recoveries,
        })
    }
}

impl autd3_rs_core::StateCheck for StateChecker {
    type Error = SoemLinkError;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        std::future::ready(StateChecker::check(self))
    }
}
