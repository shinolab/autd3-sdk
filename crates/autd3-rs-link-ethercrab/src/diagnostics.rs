use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub(crate) struct CycleDiagnostics {
    pub samples: u64,
    pub deadline_overrun: Duration,
    pub tx_rx_duration: Duration,
    pub expected_wkc: u16,
    pub working_counter: Option<u16>,
    pub all_op: Option<bool>,
    pub rx_valid: bool,
    pub tx_rx_succeeded: bool,
    pub dc_system_time_ns: Option<u64>,
    pub next_cycle_wait: Option<Duration>,
    pub cycle_start_offset: Option<Duration>,
}

pub(crate) type SharedCycleDiagnostics = Arc<Mutex<CycleDiagnostics>>;

pub(crate) fn new_shared_cycle_diagnostics() -> SharedCycleDiagnostics {
    Arc::new(Mutex::new(CycleDiagnostics::default()))
}

pub(crate) fn store_cycle_diagnostics(
    shared: &SharedCycleDiagnostics,
    diagnostics: CycleDiagnostics,
) {
    let mut guard = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = diagnostics;
}

pub(crate) fn load_cycle_diagnostics(shared: &SharedCycleDiagnostics) -> CycleDiagnostics {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
