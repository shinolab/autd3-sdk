use std::sync::Mutex;
use std::time::Duration;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use tokio::runtime::{Handle, Runtime};

enum State {
    Uninit,
    Running(Runtime),
    Shutdown,
}

static STATE: Mutex<State> = Mutex::new(State::Uninit);

pub(crate) fn handle() -> PyResult<Handle> {
    let mut state = STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if matches!(*state, State::Uninit) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to build tokio runtime: {e}")))?;
        *state = State::Running(rt);
    }
    match &*state {
        State::Running(rt) => Ok(rt.handle().clone()),
        State::Shutdown => Err(PyRuntimeError::new_err("autd3 runtime has been shut down")),
        State::Uninit => unreachable!(),
    }
}

#[pyfunction]
pub(crate) fn _shutdown_runtime(py: Python<'_>) {
    let rt = {
        let mut state = STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match std::mem::replace(&mut *state, State::Shutdown) {
            State::Running(rt) => Some(rt),
            State::Uninit | State::Shutdown => None,
        }
    };
    if let Some(rt) = rt {
        py.detach(|| rt.shutdown_timeout(Duration::from_secs(10)));
    }
}
