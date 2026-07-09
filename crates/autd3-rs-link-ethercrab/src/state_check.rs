use std::sync::{Arc, Weak};
use std::time::Duration;

use autd3_rs_core::{DeviceState, LinkStatus};
use ethercrab::MainDevice;

use crate::diagnostics::{CycleDiagnostics, SharedCycleDiagnostics, load_cycle_diagnostics};
use crate::error::EtherCrabLinkError;
use crate::status::{
    AlState, OpRecoveryAction, al_status_code_str, read_al_state, read_al_status_code,
    request_al_state,
};

pub struct StateChecker {
    maindevice: Weak<MainDevice<'static>>,
    addresses: Vec<u16>,
    states: Vec<DeviceState>,
    recoveries: u64,
    diagnostics: SharedCycleDiagnostics,
    pdu_timeout: Duration,
}

impl StateChecker {
    pub(crate) fn new(
        maindevice: &Arc<MainDevice<'static>>,
        addresses: Vec<u16>,
        diagnostics: SharedCycleDiagnostics,
        pdu_timeout: Duration,
    ) -> Self {
        Self {
            maindevice: Arc::downgrade(maindevice),
            states: vec![DeviceState::Op; addresses.len()],
            recoveries: 0,
            addresses,
            diagnostics,
            pdu_timeout,
        }
    }

    pub async fn check(&mut self) -> Result<LinkStatus, EtherCrabLinkError> {
        let Some(maindevice) = self.maindevice.upgrade() else {
            return Err(EtherCrabLinkError::Closed);
        };
        let was_all_op = self.states.iter().all(|s| *s == DeviceState::Op);
        for (device, &address) in self.addresses.iter().enumerate() {
            let (new_state, al_status_code) = Box::pin(resolve_state(
                &maindevice,
                address,
                device,
                self.pdu_timeout,
                self.states[device],
            ))
            .await;
            if new_state != self.states[device] {
                let diagnostics = load_cycle_diagnostics(&self.diagnostics);
                match new_state {
                    DeviceState::Op => tracing::info!(device, "device is back in OP"),
                    DeviceState::SafeOpError => {
                        log_safe_op_transition(
                            device,
                            al_status_code,
                            &diagnostics,
                            "device is in SAFE-OP + ERROR, acknowledged",
                        );
                    }
                    DeviceState::SafeOp => {
                        log_safe_op_transition(
                            device,
                            al_status_code,
                            &diagnostics,
                            "device is in SAFE-OP, requested OP",
                        );
                    }
                    DeviceState::Lost => tracing::warn!(device, "device is lost"),
                    DeviceState::Other(_) => {
                        tracing::warn!(device, state = %new_state, "device is in an unrecoverable state");
                    }
                }
                self.states[device] = new_state;
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
    type Error = EtherCrabLinkError;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        StateChecker::check(self)
    }
}

async fn resolve_state(
    maindevice: &MainDevice<'_>,
    address: u16,
    device: usize,
    pdu_timeout: Duration,
    previous: DeviceState,
) -> (DeviceState, Option<u16>) {
    let al = match read_al_state(maindevice, address, pdu_timeout).await {
        Ok(al) if al.is_op() => return (DeviceState::Op, None),
        Ok(al) => al,
        Err(
            e @ (ethercrab::error::Error::Timeout(_)
            | ethercrab::error::Error::WorkingCounter { .. }),
        ) => {
            tracing::trace!(device, "AL status read failed: {e}");
            return (DeviceState::Lost, None);
        }
        Err(e) => {
            tracing::error!(device, "AL status read failed: {e}");
            return (previous, None);
        }
    };

    let action = al.op_recovery_action();
    if action == OpRecoveryAction::None {
        return (DeviceState::Other(al.state_bits()), None);
    }

    let al_status_code = match read_al_status_code(maindevice, address, pdu_timeout).await {
        Ok(code) => Some(code),
        Err(e) => {
            tracing::debug!(device, "failed to read AL status code: {e}");
            None
        }
    };
    let (control, state) = match action {
        OpRecoveryAction::AckSafeOpError => (AlState::SAFE_OP_ACK, DeviceState::SafeOpError),
        OpRecoveryAction::RequestOp => (AlState::OP, DeviceState::SafeOp),
        OpRecoveryAction::None => unreachable!("handled above"),
    };
    if let Err(e) = request_al_state(maindevice, address, control, pdu_timeout).await {
        tracing::debug!(device, "failed to request AL state {control:#06x}: {e}");
    }
    (state, al_status_code)
}

fn log_safe_op_transition(
    device: usize,
    al_status_code: Option<u16>,
    diagnostics: &CycleDiagnostics,
    message: &'static str,
) {
    if let Some(code) = al_status_code {
        tracing::warn!(
            device,
            al_status_code = format_args!("{code:#06x}"),
            reason = al_status_code_str(code),
            diagnostic_samples = diagnostics.samples,
            deadline_overrun = ?diagnostics.deadline_overrun,
            tx_rx_duration = ?diagnostics.tx_rx_duration,
            expected_wkc = diagnostics.expected_wkc,
            working_counter = ?diagnostics.working_counter,
            all_op = ?diagnostics.all_op,
            rx_valid = diagnostics.rx_valid,
            tx_rx_succeeded = diagnostics.tx_rx_succeeded,
            dc_system_time_ns = ?diagnostics.dc_system_time_ns,
            next_cycle_wait = ?diagnostics.next_cycle_wait,
            cycle_start_offset = ?diagnostics.cycle_start_offset,
            "{message}",
        );
    } else {
        tracing::warn!(
            device,
            diagnostic_samples = diagnostics.samples,
            deadline_overrun = ?diagnostics.deadline_overrun,
            tx_rx_duration = ?diagnostics.tx_rx_duration,
            expected_wkc = diagnostics.expected_wkc,
            working_counter = ?diagnostics.working_counter,
            all_op = ?diagnostics.all_op,
            rx_valid = diagnostics.rx_valid,
            tx_rx_succeeded = diagnostics.tx_rx_succeeded,
            dc_system_time_ns = ?diagnostics.dc_system_time_ns,
            next_cycle_wait = ?diagnostics.next_cycle_wait,
            cycle_start_offset = ?diagnostics.cycle_start_offset,
            "{message}",
        );
    }
}
